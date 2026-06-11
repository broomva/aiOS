//! Client-tool handoff tests for the Direct tick path (BRO-1463).
//!
//! These exercise the kernel's handling of `TickInput.client_tools` —
//! tools declared by the chat client rather than the kernel's own
//! governed registry. The contract under test:
//!
//! 1. Client tools are threaded into the `ModelCompletionRequest` so
//!    the provider adapter can surface them to the model.
//! 2. When the model proposes a client tool, the kernel emits a
//!    `ToolCallRequested { category: "client" }` and hands the call
//!    back to the caller — it does NOT evaluate policy or run the
//!    harness for that call — then ends the turn in a non-`Execute`,
//!    non-`Recover` mode so the dispatch loop breaks and `RunFinished`
//!    still emits (wire: TOOL_CALL_PENDING then FINISH).
//! 3. A name collision between a client tool and a kernel registry tool
//!    resolves registry-first: the call is treated as a governed
//!    registry tool (policy + harness), never as a client handoff.
//!
//! The kernel only depends on `aios-protocol`, so the ports here are
//! self-contained in-memory mocks rather than the real `aios-events` /
//! `aios-policy` implementations (which live one layer up). The mocks
//! record whether the harness / policy gate were consulted so the
//! "client tools are never executed or gated" invariant is asserted
//! directly.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use aios_protocol::{
    ApprovalId, ApprovalPort, ApprovalRequest, ApprovalResolution, ApprovalTicket, BranchId,
    Capability, ClientToolDefinition, EventKind, EventRecord, EventRecordStream, EventStorePort,
    KernelResult, ModelCompletion, ModelCompletionRequest, ModelDirective, ModelProviderPort,
    ModelRouting, ModelStopReason, OperatingMode, PolicyGateDecision, PolicyGatePort, PolicySet,
    SessionId, ToolCall, ToolExecutionReport, ToolExecutionRequest, ToolHarnessPort, ToolOutcome,
    ToolRunId,
};
use aios_runtime::{KernelRuntime, RuntimeConfig, TickInput, TickKind};
use async_trait::async_trait;
use parking_lot::Mutex;

// ── In-memory event store ─────────────────────────────────────────────
//
// The runtime assigns sequences itself and only needs the store to
// persist + echo (append), report the per-branch head, and replay
// (read). `subscribe` is for external streaming consumers and is not on
// the tick path, so it returns an empty stream.

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

// ── Scripted model provider ───────────────────────────────────────────
//
// Records the `client_tools` of the most recent request (so the test
// can assert they were threaded into `ModelCompletionRequest`) and, on
// its first completion, optionally proposes a single tool call by name.

#[derive(Default)]
struct ScriptedProvider {
    /// Tool name to propose on the first completion (`None` → just answer).
    propose_tool: Option<String>,
    /// Registry tool to ALSO propose first (with a capability) — drives
    /// the mixed registry+client completion path.
    propose_registry_tool: Option<String>,
    /// `client_tools` seen on the most recent request.
    seen_client_tools: Mutex<Vec<ClientToolDefinition>>,
    /// Whether the first completion has already been served.
    answered: AtomicBool,
}

impl ScriptedProvider {
    fn proposing(tool_name: &str) -> Self {
        Self {
            propose_tool: Some(tool_name.to_owned()),
            ..Default::default()
        }
    }

    /// First completion proposes a capability-bearing registry tool AND
    /// a client tool, in that order (mirrors a mixed model turn).
    fn proposing_pair(registry_tool: &str, client_tool: &str) -> Self {
        Self {
            propose_tool: Some(client_tool.to_owned()),
            propose_registry_tool: Some(registry_tool.to_owned()),
            ..Default::default()
        }
    }
}

#[async_trait]
impl ModelProviderPort for ScriptedProvider {
    async fn complete(&self, request: ModelCompletionRequest) -> KernelResult<ModelCompletion> {
        *self.seen_client_tools.lock() = request.client_tools.clone();

        // Propose the tool only on the first completion; subsequent
        // ticks (if any) answer normally so loops terminate.
        let first = !self.answered.swap(true, Ordering::SeqCst);
        if first && let Some(tool_name) = self.propose_tool.clone() {
            let mut directives = Vec::new();
            if let Some(registry_tool) = self.propose_registry_tool.clone() {
                directives.push(ModelDirective::ToolCall {
                    call: ToolCall {
                        call_id: "call-0".to_owned(),
                        tool_name: registry_tool,
                        input: serde_json::json!({ "path": "artifacts/x" }),
                        requested_capabilities: vec![Capability::fs_write("/session/artifacts/**")],
                    },
                });
            }
            directives.push(ModelDirective::ToolCall {
                call: ToolCall {
                    call_id: "call-1".to_owned(),
                    tool_name,
                    input: serde_json::json!({ "q": "berlin" }),
                    requested_capabilities: Vec::new(),
                },
            });
            return Ok(ModelCompletion {
                provider: "scripted".to_owned(),
                model: "scripted-deterministic".to_owned(),
                llm_call_record: None,
                directives,
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

// ── Recording tool harness ────────────────────────────────────────────
//
// Flips `executed` the moment `execute` is called, so a test can assert
// the harness was (or was NOT) consulted. Returns a trivial success so
// registry-tool paths still complete.

#[derive(Default)]
struct RecordingHarness {
    executed: AtomicBool,
}

#[async_trait]
impl ToolHarnessPort for RecordingHarness {
    async fn execute(&self, request: ToolExecutionRequest) -> KernelResult<ToolExecutionReport> {
        self.executed.store(true, Ordering::SeqCst);
        Ok(ToolExecutionReport {
            tool_run_id: ToolRunId::default(),
            call_id: request.call.call_id.clone(),
            tool_name: request.call.tool_name.clone(),
            exit_status: 0,
            duration_ms: 0,
            outcome: ToolOutcome::Success {
                output: serde_json::json!({ "ok": true }),
            },
        })
    }
}

// ── Recording policy gate ─────────────────────────────────────────────
//
// Flips `evaluated` when `evaluate` is called. Allows every capability
// so governed registry-tool paths proceed; the flag lets a test assert
// that client-tool calls never reach the gate.

#[derive(Default)]
struct RecordingPolicy {
    evaluated: AtomicBool,
    /// When set, every requested capability requires approval — drives
    /// the AskHuman path for governed registry tools.
    require_approval: bool,
}

#[async_trait]
impl PolicyGatePort for RecordingPolicy {
    async fn evaluate(
        &self,
        _session_id: SessionId,
        requested: Vec<Capability>,
    ) -> KernelResult<PolicyGateDecision> {
        self.evaluated.store(true, Ordering::SeqCst);
        if self.require_approval {
            return Ok(PolicyGateDecision {
                allowed: Vec::new(),
                requires_approval: requested,
                denied: Vec::new(),
            });
        }
        Ok(PolicyGateDecision {
            allowed: requested,
            requires_approval: Vec::new(),
            denied: Vec::new(),
        })
    }
}

// ── No-op approval port ───────────────────────────────────────────────

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

// ── Harness wiring ────────────────────────────────────────────────────

/// The mock ports a test wants to inspect after driving a tick.
struct Ports {
    provider: Arc<ScriptedProvider>,
    harness: Arc<RecordingHarness>,
    policy: Arc<RecordingPolicy>,
}

fn build_runtime(
    provider: ScriptedProvider,
    registry_tool_names: Vec<&str>,
) -> (KernelRuntime, Ports) {
    build_runtime_with_policy(provider, registry_tool_names, RecordingPolicy::default())
}

fn build_runtime_with_policy(
    provider: ScriptedProvider,
    registry_tool_names: Vec<&str>,
    policy: RecordingPolicy,
) -> (KernelRuntime, Ports) {
    let root = std::env::temp_dir().join(format!(
        "aios-runtime-client-tools-{}",
        uuid::Uuid::new_v4()
    ));
    let event_store: Arc<dyn EventStorePort> = Arc::new(MemEventStore::default());

    let provider = Arc::new(provider);
    let harness = Arc::new(RecordingHarness::default());
    let policy = Arc::new(policy);

    let runtime = KernelRuntime::new(
        RuntimeConfig::new(root),
        event_store,
        provider.clone() as Arc<dyn ModelProviderPort>,
        harness.clone() as Arc<dyn ToolHarnessPort>,
        Arc::new(NoopApprovals) as Arc<dyn ApprovalPort>,
        policy.clone() as Arc<dyn PolicyGatePort>,
    )
    .with_registry_tool_names(registry_tool_names);

    (
        runtime,
        Ports {
            provider,
            harness,
            policy,
        },
    )
}

fn client_tool(name: &str) -> ClientToolDefinition {
    ClientToolDefinition {
        name: name.to_owned(),
        description: format!("client tool {name}"),
        parameters: serde_json::json!({ "type": "object" }),
    }
}

async fn tick_with_client_tools(
    runtime: &KernelRuntime,
    session: &SessionId,
    client_tools: Vec<ClientToolDefinition>,
) -> aios_runtime::TickOutput {
    runtime
        .tick_on_branch(
            session,
            &BranchId::main(),
            TickInput {
                objective: "look up the weather".to_owned(),
                proposed_tool: None,
                system_prompt: None,
                allowed_tools: None,
                client_tools,
                kind: TickKind::Direct,
            },
        )
        .await
        .expect("tick succeeds")
}

async fn new_session(runtime: &KernelRuntime) -> SessionId {
    let manifest = runtime
        .create_session("test-owner", PolicySet::default(), ModelRouting::default())
        .await
        .expect("create session");
    manifest.session_id
}

fn count_kinds(events: &[EventRecord]) -> HashMap<&'static str, usize> {
    let mut counts = HashMap::new();
    for record in events {
        let name = match &record.kind {
            EventKind::ToolCallRequested { .. } => "ToolCallRequested",
            EventKind::ToolCallStarted { .. } => "ToolCallStarted",
            EventKind::ToolCallCompleted { .. } => "ToolCallCompleted",
            EventKind::ToolCallFailed { .. } => "ToolCallFailed",
            EventKind::RunFinished { .. } => "RunFinished",
            EventKind::RunErrored { .. } => "RunErrored",
            _ => continue,
        };
        *counts.entry(name).or_insert(0) += 1;
    }
    counts
}

// ── Tests ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn tick_threads_client_tools_into_model_request() {
    // A tick carrying `client_tools` must surface them on the
    // `ModelCompletionRequest` the provider receives.
    let (runtime, ports) = build_runtime(ScriptedProvider::default(), Vec::new());
    let session = new_session(&runtime).await;

    let tools = vec![client_tool("get_weather"), client_tool("get_time")];
    let _ = tick_with_client_tools(&runtime, &session, tools.clone()).await;

    let seen = ports.provider.seen_client_tools.lock().clone();
    assert_eq!(
        seen, tools,
        "provider request should carry the client tools"
    );
}

#[tokio::test]
async fn client_tool_call_is_handed_back_not_executed() {
    // The model proposes a client-declared tool. The kernel must:
    //  - emit ToolCallRequested { category: "client" },
    //  - NOT run the harness for it,
    //  - NOT evaluate policy for it,
    //  - end the turn in a mode that breaks the Execute-continue loop
    //    and is not Recover, while still emitting RunFinished.
    let (runtime, ports) = build_runtime(ScriptedProvider::proposing("get_weather"), Vec::new());
    let session = new_session(&runtime).await;

    let output = tick_with_client_tools(&runtime, &session, vec![client_tool("get_weather")]).await;

    // Mode breaks the dispatch loop (≠ Execute) and is not an error (≠ Recover).
    assert_ne!(
        output.mode,
        OperatingMode::Execute,
        "client-tool turn must not stay in Execute (loop would not break)"
    );
    assert_ne!(
        output.mode,
        OperatingMode::Recover,
        "client-tool handoff is not an error"
    );

    // The harness and policy gate are never consulted for a client tool.
    assert!(
        !ports.harness.executed.load(Ordering::SeqCst),
        "harness must NOT execute a client tool"
    );
    assert!(
        !ports.policy.evaluated.load(Ordering::SeqCst),
        "policy gate must NOT be evaluated for a client tool"
    );

    // The journal carries the client handoff and a clean finish, and no
    // execution events (Started/Completed/Failed).
    let events = runtime
        .read_events(&session, 0, 1024)
        .await
        .expect("read events");
    let counts = count_kinds(&events);
    assert_eq!(
        counts.get("ToolCallRequested").copied().unwrap_or(0),
        1,
        "exactly one ToolCallRequested (the client handoff)"
    );
    assert_eq!(
        counts.get("ToolCallStarted").copied().unwrap_or(0),
        0,
        "no ToolCallStarted — client tools are not executed"
    );
    assert_eq!(
        counts.get("ToolCallCompleted").copied().unwrap_or(0),
        0,
        "no ToolCallCompleted — client tools are not executed"
    );
    assert_eq!(
        counts.get("RunFinished").copied().unwrap_or(0),
        1,
        "RunFinished still emits after the client handoff"
    );
    assert_eq!(
        counts.get("RunErrored").copied().unwrap_or(0),
        0,
        "client handoff is not an error"
    );

    // The single ToolCallRequested is tagged category "client".
    let requested = events
        .iter()
        .find_map(|r| match &r.kind {
            EventKind::ToolCallRequested {
                tool_name,
                category,
                ..
            } => Some((tool_name.clone(), category.clone())),
            _ => None,
        })
        .expect("a ToolCallRequested event");
    assert_eq!(requested.0, "get_weather");
    assert_eq!(requested.1.as_deref(), Some("client"));
}

#[tokio::test]
async fn registry_tool_wins_name_collision() {
    // A client declares a tool whose name is also a kernel registry
    // tool. The registry wins: the proposed call is treated as a
    // governed registry tool (policy + harness run) and is NOT handed
    // back to the client as a category-"client" handoff.
    let (runtime, ports) = build_runtime(
        ScriptedProvider::proposing("get_weather"),
        vec!["get_weather"],
    );
    let session = new_session(&runtime).await;

    let _ = tick_with_client_tools(&runtime, &session, vec![client_tool("get_weather")]).await;

    // Registry path: the harness executes and policy is evaluated.
    assert!(
        ports.harness.executed.load(Ordering::SeqCst),
        "registry tool must execute through the harness on a collision"
    );
    assert!(
        ports.policy.evaluated.load(Ordering::SeqCst),
        "registry tool must be policy-evaluated on a collision"
    );

    // The ToolCallRequested for a registry tool is NOT category "client".
    let events = runtime
        .read_events(&session, 0, 1024)
        .await
        .expect("read events");
    let category = events.iter().find_map(|r| match &r.kind {
        EventKind::ToolCallRequested { category, .. } => Some(category.clone()),
        _ => None,
    });
    assert_ne!(
        category.flatten().as_deref(),
        Some("client"),
        "a registry tool must not be tagged as a client handoff"
    );
}

#[tokio::test]
async fn mixed_completion_preserves_ask_human_over_client_handoff() {
    // A mixed first completion: a governed registry tool whose
    // capability requires approval, THEN a client tool. The client-tool
    // handoff forces `Sleep` for plain turns — but it must NOT clobber
    // `AskHuman`, or the host loses the only pending-approval signal
    // and the approval stalls silently on the next dispatch.
    let (runtime, _ports) = build_runtime_with_policy(
        ScriptedProvider::proposing_pair("fs.write", "get_weather"),
        vec!["fs.write"],
        RecordingPolicy {
            require_approval: true,
            ..Default::default()
        },
    );
    let session = new_session(&runtime).await;

    let output = tick_with_client_tools(&runtime, &session, vec![client_tool("get_weather")]).await;

    assert_eq!(
        output.mode,
        OperatingMode::AskHuman,
        "pending approval must win over the client-tool Sleep forcing"
    );
}
