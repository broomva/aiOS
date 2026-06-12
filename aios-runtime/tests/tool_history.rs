//! Tool-transcript rendering in conversation history (BRO-1465).
//!
//! `build_conversation_history` must render registry tool activity —
//! `ToolCallRequested` / `ToolCallCompleted` / `ToolCallFailed` — into the
//! assistant turns of the reconstructed history. Without it the model has
//! no evidence its tools ever ran, observed live (2026-06-12 receipt) as:
//!
//! * gpt-5-mini re-calling a SUCCESSFUL `write_file` on every continuation
//!   tick (it could not see the result), and
//! * denial dead-air — a post-`Recover` wrap-up call had nothing to
//!   verbalize because `ToolCallFailed` never reached the prompt.
//!
//! Contract under test:
//! 1. After a successful registry tool call, the NEXT tick's
//!    `ModelCompletionRequest.conversation_history` contains
//!    `[tool_call <name>(<args>)]` and `[tool_result <name> ok: …]`.
//! 2. Oversized results are truncated with an elision marker.
//! 3. After a policy denial, the next tick's history contains
//!    `[tool_result <name> failed: capabilities denied…]`.
//! 4. Client-category tool calls are NOT transcribed (their results return
//!    as ordinary conversation turns on the next dispatch).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use aios_protocol::{
    ApprovalId, ApprovalPort, ApprovalRequest, ApprovalResolution, ApprovalTicket, BranchId,
    Capability, ClientToolDefinition, EventRecord, EventRecordStream, EventStorePort, KernelResult,
    ModelCompletion, ModelCompletionRequest, ModelDirective, ModelProviderPort, ModelRouting,
    ModelStopReason, PolicyGateDecision, PolicyGatePort, PolicySet, SessionId, ToolCall,
    ToolExecutionReport, ToolExecutionRequest, ToolHarnessPort, ToolOutcome, ToolRunId,
};
use aios_runtime::{KernelRuntime, RuntimeConfig, TickInput, TickKind};
use async_trait::async_trait;
use parking_lot::Mutex;

// ── In-memory event store ─────────────────────────────────────────────

#[derive(Default)]
struct MemEventStore {
    events: Mutex<Vec<EventRecord>>,
}

#[async_trait]
impl EventStorePort for MemEventStore {
    async fn append(&self, event: EventRecord) -> KernelResult<EventRecord> {
        self.events.lock().push(event.clone());
        Ok(event)
    }

    async fn read(
        &self,
        session_id: SessionId,
        branch_id: BranchId,
        from_sequence: u64,
        limit: usize,
    ) -> KernelResult<Vec<EventRecord>> {
        let events = self.events.lock();
        Ok(events
            .iter()
            .filter(|e| {
                e.session_id == session_id
                    && e.branch_id == branch_id
                    && e.sequence >= from_sequence
            })
            .take(limit)
            .cloned()
            .collect())
    }

    async fn head(&self, session_id: SessionId, branch_id: BranchId) -> KernelResult<u64> {
        let events = self.events.lock();
        Ok(events
            .iter()
            .filter(|e| e.session_id == session_id && e.branch_id == branch_id)
            .map(|e| e.sequence)
            .max()
            .unwrap_or(0))
    }

    async fn subscribe(
        &self,
        _session_id: SessionId,
        _branch_id: BranchId,
        _after_sequence: u64,
    ) -> KernelResult<EventRecordStream> {
        Ok(Box::pin(futures_util::stream::empty()))
    }
}

// ── History-recording provider ────────────────────────────────────────
//
// Records `conversation_history` of every request. The first completion
// proposes one tool call (registry or client per the test); subsequent
// completions answer plainly so loops terminate.

struct HistoryProvider {
    propose_tool: Option<ToolCall>,
    histories: Mutex<Vec<Vec<aios_protocol::ConversationTurn>>>,
    answered: AtomicBool,
}

impl HistoryProvider {
    fn proposing(call: ToolCall) -> Self {
        Self {
            propose_tool: Some(call),
            histories: Mutex::new(Vec::new()),
            answered: AtomicBool::new(false),
        }
    }

    /// All assistant-turn content seen by request `index`, joined.
    fn assistant_text_at(&self, index: usize) -> String {
        self.histories.lock()[index]
            .iter()
            .filter(|t| t.role == "assistant")
            .map(|t| t.content.clone())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn request_count(&self) -> usize {
        self.histories.lock().len()
    }
}

#[async_trait]
impl ModelProviderPort for HistoryProvider {
    async fn complete(&self, request: ModelCompletionRequest) -> KernelResult<ModelCompletion> {
        self.histories
            .lock()
            .push(request.conversation_history.clone());

        let first = !self.answered.swap(true, Ordering::SeqCst);
        if first && let Some(call) = self.propose_tool.clone() {
            return Ok(ModelCompletion {
                provider: "scripted".to_owned(),
                model: "scripted-deterministic".to_owned(),
                llm_call_record: None,
                directives: vec![ModelDirective::ToolCall { call }],
                stop_reason: ModelStopReason::ToolCall,
                usage: None,
                final_answer: None,
            });
        }

        Ok(ModelCompletion {
            provider: "scripted".to_owned(),
            model: "scripted-deterministic".to_owned(),
            llm_call_record: None,
            directives: vec![ModelDirective::Message {
                role: "assistant".to_owned(),
                content: "done".to_owned(),
            }],
            stop_reason: ModelStopReason::Completed,
            usage: None,
            final_answer: Some("done".to_owned()),
        })
    }
}

// ── Harness with configurable result size ─────────────────────────────

struct SizedResultHarness {
    result_payload: serde_json::Value,
}

#[async_trait]
impl ToolHarnessPort for SizedResultHarness {
    async fn execute(&self, request: ToolExecutionRequest) -> KernelResult<ToolExecutionReport> {
        Ok(ToolExecutionReport {
            tool_run_id: ToolRunId::default(),
            call_id: request.call.call_id.clone(),
            tool_name: request.call.tool_name.clone(),
            exit_status: 0,
            duration_ms: 0,
            outcome: ToolOutcome::Success {
                output: self.result_payload.clone(),
            },
        })
    }
}

// ── Policy gate: allow-all or deny-all ────────────────────────────────

struct StaticGate {
    deny_all: bool,
}

#[async_trait]
impl PolicyGatePort for StaticGate {
    async fn evaluate(
        &self,
        _session_id: SessionId,
        requested: Vec<Capability>,
    ) -> KernelResult<PolicyGateDecision> {
        if self.deny_all {
            return Ok(PolicyGateDecision {
                allowed: Vec::new(),
                requires_approval: Vec::new(),
                denied: requested,
            });
        }
        Ok(PolicyGateDecision {
            allowed: requested,
            requires_approval: Vec::new(),
            denied: Vec::new(),
        })
    }
}

// ── No-op approvals ───────────────────────────────────────────────────

#[derive(Default)]
struct NoopApprovals;

#[async_trait]
impl ApprovalPort for NoopApprovals {
    async fn enqueue(&self, request: ApprovalRequest) -> KernelResult<ApprovalTicket> {
        Ok(ApprovalTicket {
            approval_id: ApprovalId::default(),
            session_id: request.session_id,
            call_id: request.call_id,
            tool_name: request.tool_name,
            capability: request.capability,
            reason: request.reason,
            created_at: chrono::Utc::now(),
        })
    }

    async fn list_pending(&self, _session_id: SessionId) -> KernelResult<Vec<ApprovalTicket>> {
        Ok(Vec::new())
    }

    async fn resolve(
        &self,
        approval_id: ApprovalId,
        approved: bool,
        actor: String,
    ) -> KernelResult<ApprovalResolution> {
        Ok(ApprovalResolution {
            approval_id,
            approved,
            actor,
            resolved_at: chrono::Utc::now(),
        })
    }
}

// ── Wiring ────────────────────────────────────────────────────────────

fn registry_call(tool_name: &str) -> ToolCall {
    ToolCall {
        call_id: "call-0".to_owned(),
        tool_name: tool_name.to_owned(),
        input: serde_json::json!({ "path": "artifacts/receipt.txt", "content": "hi" }),
        requested_capabilities: vec![Capability::fs_write("/session/artifacts/**")],
    }
}

fn build_runtime(
    provider: HistoryProvider,
    harness_result: serde_json::Value,
    deny_all: bool,
) -> (KernelRuntime, Arc<HistoryProvider>) {
    let root = std::env::temp_dir().join(format!(
        "aios-runtime-tool-history-{}",
        uuid::Uuid::new_v4()
    ));
    let provider = Arc::new(provider);

    let runtime = KernelRuntime::new(
        RuntimeConfig::new(root),
        Arc::new(MemEventStore::default()) as Arc<dyn EventStorePort>,
        provider.clone() as Arc<dyn ModelProviderPort>,
        Arc::new(SizedResultHarness {
            result_payload: harness_result,
        }) as Arc<dyn ToolHarnessPort>,
        Arc::new(NoopApprovals) as Arc<dyn ApprovalPort>,
        Arc::new(StaticGate { deny_all }) as Arc<dyn PolicyGatePort>,
    )
    .with_registry_tool_names(vec!["write_file"]);

    (runtime, provider)
}

async fn tick(
    runtime: &KernelRuntime,
    session: &SessionId,
    objective: &str,
    client_tools: Vec<ClientToolDefinition>,
) {
    runtime
        .tick_on_branch(
            session,
            &BranchId::main(),
            TickInput {
                objective: objective.to_owned(),
                proposed_tool: None,
                system_prompt: None,
                allowed_tools: None,
                client_tools,
                kind: TickKind::Direct,
            },
        )
        .await
        .expect("tick");
}

async fn new_session(runtime: &KernelRuntime) -> SessionId {
    let session = SessionId::default();
    runtime
        .create_session_with_id(
            session.clone(),
            "tool-history-test",
            PolicySet::default(),
            ModelRouting::default(),
        )
        .await
        .expect("create session");
    session
}

// ── Tests ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn successful_tool_call_renders_transcript_in_next_tick_history() {
    let (runtime, provider) = build_runtime(
        HistoryProvider::proposing(registry_call("write_file")),
        serde_json::json!({ "success": true, "path": "artifacts/receipt.txt" }),
        false,
    );
    let session = new_session(&runtime).await;

    tick(&runtime, &session, "create the receipt file", Vec::new()).await;
    tick(&runtime, &session, "", Vec::new()).await;

    assert_eq!(provider.request_count(), 2);
    let history_text = provider.assistant_text_at(1);
    assert!(
        history_text.contains("[tool_call write_file("),
        "second request must show the tool call; got: {history_text}"
    );
    assert!(
        history_text.contains("[tool_result write_file ok:"),
        "second request must show the tool result; got: {history_text}"
    );
    assert!(
        history_text.contains("artifacts/receipt.txt"),
        "result payload (path) must be visible for dedup awareness; got: {history_text}"
    );
}

#[tokio::test]
async fn oversized_tool_result_is_truncated_in_history() {
    let big = "x".repeat(5000);
    let (runtime, provider) = build_runtime(
        HistoryProvider::proposing(registry_call("write_file")),
        serde_json::json!({ "success": true, "blob": big }),
        false,
    );
    let session = new_session(&runtime).await;

    tick(&runtime, &session, "create it", Vec::new()).await;
    tick(&runtime, &session, "", Vec::new()).await;

    let history_text = provider.assistant_text_at(1);
    assert!(
        history_text.contains("(truncated)"),
        "oversized result must carry the elision marker; got len {}",
        history_text.len()
    );
    // Budget is 1200 chars for the result body — the rendered turn must be
    // bounded well below the raw 5000-char payload.
    assert!(
        history_text.len() < 3000,
        "rendered history must be bounded; got len {}",
        history_text.len()
    );
}

#[tokio::test]
async fn denied_tool_call_renders_failure_in_next_tick_history() {
    let (runtime, provider) = build_runtime(
        HistoryProvider::proposing(registry_call("write_file")),
        serde_json::json!({ "unused": true }),
        true, // deny_all
    );
    let session = new_session(&runtime).await;

    tick(&runtime, &session, "create the receipt file", Vec::new()).await;
    tick(&runtime, &session, "", Vec::new()).await;

    let history_text = provider.assistant_text_at(1);
    assert!(
        history_text.contains("[tool_result write_file failed: capabilities denied"),
        "wrap-up call must see the denial; got: {history_text}"
    );
}

#[tokio::test]
async fn client_tool_calls_are_not_transcribed() {
    // Propose a CLIENT tool (not in the registry; declared via TickInput).
    let client_call = ToolCall {
        call_id: "call-c".to_owned(),
        tool_name: "web_search".to_owned(),
        input: serde_json::json!({ "q": "berlin" }),
        requested_capabilities: Vec::new(),
    };
    let (runtime, provider) = build_runtime(
        HistoryProvider::proposing(client_call),
        serde_json::json!({ "unused": true }),
        false,
    );
    let session = new_session(&runtime).await;

    let client_tools = vec![ClientToolDefinition {
        name: "web_search".to_owned(),
        description: "client web search".to_owned(),
        parameters: serde_json::json!({ "type": "object" }),
    }];
    tick(&runtime, &session, "search berlin", client_tools.clone()).await;
    tick(&runtime, &session, "", client_tools).await;

    let history_text = provider.assistant_text_at(1);
    assert!(
        !history_text.contains("[tool_call web_search"),
        "client-category calls must not be transcribed (their results return \
         as conversation turns on the next dispatch); got: {history_text}"
    );
}
