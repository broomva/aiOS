use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use aios_events::EventJournal;
use aios_memory::{MemoryStore, extract_observation};
use aios_model::{
    AgentStateVector, BudgetState, CheckpointId, CheckpointManifest, EventKind, EventRecord,
    LoopPhase, ModelRouting, OperatingMode, PolicySet, RiskLevel, SessionId, SessionManifest,
    ToolCall, ToolOutcome,
};
use aios_policy::{ApprovalQueue, SessionPolicyEngine};
use aios_tools::{DispatchResult, ToolContext, ToolDispatcher, ToolExecutionReport};
use anyhow::{Context, Result, bail};
use chrono::Utc;
use parking_lot::Mutex;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::fs;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub root: PathBuf,
    pub checkpoint_every_ticks: u64,
    pub circuit_breaker_errors: u32,
}

impl RuntimeConfig {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            checkpoint_every_ticks: 1,
            circuit_breaker_errors: 3,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TickInput {
    pub objective: String,
    pub proposed_tool: Option<ToolCall>,
}

#[derive(Debug, Clone)]
pub struct TickOutput {
    pub session_id: SessionId,
    pub mode: OperatingMode,
    pub state: AgentStateVector,
    pub events_emitted: u64,
    pub last_sequence: u64,
}

#[derive(Debug, Clone)]
struct SessionRuntimeState {
    manifest: SessionManifest,
    next_sequence: u64,
    tick_count: u64,
    mode: OperatingMode,
    state_vector: AgentStateVector,
}

#[derive(Clone)]
pub struct KernelRuntime {
    config: RuntimeConfig,
    journal: EventJournal,
    dispatcher: ToolDispatcher,
    memory: Arc<dyn MemoryStore>,
    approvals: ApprovalQueue,
    session_policy: Arc<SessionPolicyEngine>,
    sessions: Arc<Mutex<HashMap<SessionId, SessionRuntimeState>>>,
}

impl KernelRuntime {
    pub fn new(
        config: RuntimeConfig,
        journal: EventJournal,
        dispatcher: ToolDispatcher,
        memory: Arc<dyn MemoryStore>,
        approvals: ApprovalQueue,
        session_policy: Arc<SessionPolicyEngine>,
    ) -> Self {
        Self {
            config,
            journal,
            dispatcher,
            memory,
            approvals,
            session_policy,
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn create_session(
        &self,
        owner: impl Into<String>,
        policy: PolicySet,
        model_routing: ModelRouting,
    ) -> Result<SessionManifest> {
        let session_id = SessionId::new();
        let owner = owner.into();
        let session_root = self.session_root(session_id);
        self.initialize_workspace(session_root.as_path()).await?;

        let manifest = SessionManifest {
            session_id,
            owner,
            created_at: Utc::now(),
            workspace_root: session_root.to_string_lossy().into_owned(),
            model_routing,
            policy,
        };

        self.write_pretty_json(session_root.join("manifest.json"), &manifest)
            .await?;

        let manifest_hash = sha256_json(&manifest)?;

        let latest_sequence = self.journal.latest_sequence(session_id).await.unwrap_or(0);
        self.sessions.lock().insert(
            session_id,
            SessionRuntimeState {
                manifest: manifest.clone(),
                next_sequence: latest_sequence + 1,
                tick_count: 0,
                mode: OperatingMode::Explore,
                state_vector: AgentStateVector::default(),
            },
        );
        self.session_policy
            .set_policy(session_id, &manifest.policy)
            .await;

        self.append_event(session_id, EventKind::SessionCreated { manifest_hash })
            .await?;

        self.emit_phase(session_id, LoopPhase::Sleep).await?;

        Ok(manifest)
    }

    pub async fn tick(&self, session_id: SessionId, input: TickInput) -> Result<TickOutput> {
        let (manifest, mut state) = {
            let sessions = self.sessions.lock();
            let session = sessions
                .get(&session_id)
                .with_context(|| format!("session not found: {}", session_id.0))?;
            (session.manifest.clone(), session.state_vector.clone())
        };

        let mut emitted = 0_u64;

        emitted += self.emit_phase(session_id, LoopPhase::Perceive).await?;
        emitted += self.emit_phase(session_id, LoopPhase::Deliberate).await?;

        self.append_event(
            session_id,
            EventKind::DeliberationProposed {
                summary: input.objective.clone(),
                proposed_tool: input.proposed_tool.clone(),
            },
        )
        .await?;
        emitted += 1;

        let pending_approvals = self.approvals.pending_for_session(session_id).await;
        let mut mode = self.estimate_mode(&state, pending_approvals.len());

        self.append_event(
            session_id,
            EventKind::StateEstimated {
                state: state.clone(),
                mode: mode.clone(),
            },
        )
        .await?;
        emitted += 1;

        if matches!(mode, OperatingMode::AskHuman | OperatingMode::Sleep) {
            emitted += self
                .finalize_tick(session_id, &manifest, &mut state, &mode)
                .await?;
            return self
                .current_tick_output(session_id, mode, state, emitted)
                .await;
        }

        if let Some(call) = input.proposed_tool {
            emitted += self.emit_phase(session_id, LoopPhase::Gate).await?;
            self.append_event(
                session_id,
                EventKind::ToolCallRequested { call: call.clone() },
            )
            .await?;
            emitted += 1;

            let context = ToolContext {
                workspace_root: PathBuf::from(&manifest.workspace_root),
            };

            match self
                .dispatcher
                .dispatch(session_id, &context, call.clone())
                .await
            {
                Ok(DispatchResult::NeedsApproval { evaluation, .. }) => {
                    mode = OperatingMode::AskHuman;
                    for capability in evaluation.requires_approval {
                        let ticket = self
                            .approvals
                            .enqueue(
                                session_id,
                                capability.clone(),
                                format!("approval required for tool {}", call.tool_name),
                            )
                            .await;
                        self.append_event(
                            session_id,
                            EventKind::ApprovalRequested {
                                approval_id: ticket.approval_id,
                                reason: ticket.reason,
                                capability,
                            },
                        )
                        .await?;
                        emitted += 1;
                    }
                }
                Ok(DispatchResult::Executed(report)) => {
                    emitted += self.emit_phase(session_id, LoopPhase::Execute).await?;
                    emitted += self
                        .record_tool_report(session_id, &manifest, &report)
                        .await?;
                    self.apply_homeostasis_controllers(&mut state, &report);
                    mode = self.estimate_mode(&state, 0);
                }
                Err(error) => {
                    state.error_streak += 1;
                    state.uncertainty = (state.uncertainty + 0.15).min(1.0);
                    state.budget.error_budget_remaining =
                        state.budget.error_budget_remaining.saturating_sub(1);
                    mode = OperatingMode::Recover;

                    self.append_event(
                        session_id,
                        EventKind::ErrorRaised {
                            message: error.to_string(),
                        },
                    )
                    .await?;
                    emitted += 1;
                }
            }
        }

        if state.error_streak >= self.config.circuit_breaker_errors {
            mode = OperatingMode::Recover;
            self.append_event(
                session_id,
                EventKind::CircuitBreakerTripped {
                    reason: "error streak exceeded threshold".to_owned(),
                    error_streak: state.error_streak,
                },
            )
            .await?;
            emitted += 1;
        }

        emitted += self
            .finalize_tick(session_id, &manifest, &mut state, &mode)
            .await?;
        self.current_tick_output(session_id, mode, state, emitted)
            .await
    }

    pub async fn resolve_approval(
        &self,
        session_id: SessionId,
        approval_id: Uuid,
        approved: bool,
        actor: impl Into<String>,
    ) -> Result<()> {
        let actor = actor.into();
        let resolution = self
            .approvals
            .resolve(approval_id, approved, actor.clone())
            .await
            .with_context(|| format!("approval not pending: {approval_id}"))?;

        self.append_event(
            session_id,
            EventKind::ApprovalResolved {
                approval_id,
                approved: resolution.approved,
                actor,
            },
        )
        .await?;
        Ok(())
    }

    pub fn subscribe_events(&self) -> tokio::sync::broadcast::Receiver<EventRecord> {
        self.journal.subscribe()
    }

    pub async fn record_external_event(
        &self,
        session_id: SessionId,
        kind: EventKind,
    ) -> Result<()> {
        {
            let sessions = self.sessions.lock();
            if !sessions.contains_key(&session_id) {
                bail!("session not found: {}", session_id.0);
            }
        }
        self.append_event(session_id, kind).await
    }

    pub async fn read_events(
        &self,
        session_id: SessionId,
        from_sequence: u64,
        limit: usize,
    ) -> Result<Vec<EventRecord>> {
        self.journal
            .read_from(session_id, from_sequence, limit)
            .await
    }

    fn estimate_mode(&self, state: &AgentStateVector, pending_approvals: usize) -> OperatingMode {
        if pending_approvals > 0 {
            return OperatingMode::AskHuman;
        }

        if state.error_streak >= self.config.circuit_breaker_errors {
            return OperatingMode::Recover;
        }

        if state.progress >= 0.98 {
            return OperatingMode::Sleep;
        }

        if state.context_pressure > 0.8 || state.uncertainty > 0.65 {
            return OperatingMode::Explore;
        }

        if state.side_effect_pressure > 0.6 {
            return OperatingMode::Verify;
        }

        OperatingMode::Execute
    }

    fn apply_homeostasis_controllers(
        &self,
        state: &mut AgentStateVector,
        report: &ToolExecutionReport,
    ) {
        state.budget.tool_calls_remaining = state.budget.tool_calls_remaining.saturating_sub(1);
        state.budget.tokens_remaining = state.budget.tokens_remaining.saturating_sub(750);
        state.budget.time_remaining_ms = state.budget.time_remaining_ms.saturating_sub(1200);

        if report.exit_status == 0 {
            state.progress = (state.progress + 0.12).min(1.0);
            state.uncertainty = (state.uncertainty * 0.85).max(0.05);
            state.error_streak = 0;
            state.side_effect_pressure = (state.side_effect_pressure + 0.2).min(1.0);
        } else {
            state.error_streak += 1;
            state.uncertainty = (state.uncertainty + 0.18).min(1.0);
            state.budget.error_budget_remaining =
                state.budget.error_budget_remaining.saturating_sub(1);
            state.side_effect_pressure = (state.side_effect_pressure * 0.5).max(0.1);
        }

        state.context_pressure = (state.context_pressure + 0.03).min(1.0);
        state.human_dependency = if state.error_streak >= 2 { 0.6 } else { 0.0 };

        state.risk_level = if state.uncertainty > 0.75 || state.side_effect_pressure > 0.7 {
            RiskLevel::High
        } else if state.uncertainty > 0.45 || state.side_effect_pressure > 0.4 {
            RiskLevel::Medium
        } else {
            RiskLevel::Low
        };
    }

    async fn finalize_tick(
        &self,
        session_id: SessionId,
        manifest: &SessionManifest,
        state: &mut AgentStateVector,
        mode: &OperatingMode,
    ) -> Result<u64> {
        let mut emitted = 0_u64;

        emitted += self.emit_phase(session_id, LoopPhase::Reflect).await?;

        self.append_event(
            session_id,
            EventKind::BudgetUpdated {
                budget: state.budget.clone(),
                reason: "tick accounting".to_owned(),
            },
        )
        .await?;
        emitted += 1;

        self.append_event(
            session_id,
            EventKind::StateEstimated {
                state: state.clone(),
                mode: mode.clone(),
            },
        )
        .await?;
        emitted += 1;

        let checkpoint_id = if self.should_checkpoint(session_id)? {
            let checkpoint = self.create_checkpoint(session_id, manifest, state).await?;
            self.append_event(
                session_id,
                EventKind::CheckpointCreated {
                    checkpoint_id: checkpoint.checkpoint_id,
                    event_sequence: checkpoint.event_sequence,
                    state_hash: checkpoint.state_hash,
                },
            )
            .await?;
            emitted += 1;
            Some(checkpoint.checkpoint_id)
        } else {
            None
        };

        self.write_heartbeat(session_id, state, mode).await?;
        self.append_event(
            session_id,
            EventKind::Heartbeat {
                summary: "tick complete".to_owned(),
                checkpoint_id,
            },
        )
        .await?;
        emitted += 1;

        emitted += self.emit_phase(session_id, LoopPhase::Sleep).await?;

        self.persist_runtime_state(session_id, state.clone(), mode.clone())?;

        Ok(emitted)
    }

    async fn record_tool_report(
        &self,
        session_id: SessionId,
        manifest: &SessionManifest,
        report: &ToolExecutionReport,
    ) -> Result<u64> {
        let mut emitted = 0;

        self.append_event(
            session_id,
            EventKind::ToolCallStarted {
                tool_run_id: report.tool_run_id,
                tool_name: report.tool_name.clone(),
            },
        )
        .await?;
        emitted += 1;

        self.append_event(
            session_id,
            EventKind::ToolCallCompleted {
                tool_run_id: report.tool_run_id,
                exit_status: report.exit_status,
                outcome: report.outcome.clone(),
            },
        )
        .await?;
        emitted += 1;

        if let ToolOutcome::Success { output } = &report.outcome
            && let Some(path) = output.get("path").and_then(|v| v.as_str())
        {
            let full_path =
                PathBuf::from(&manifest.workspace_root).join(path.trim_start_matches('/'));
            let content_hash = if fs::try_exists(&full_path).await.unwrap_or(false) {
                let data = fs::read(&full_path).await?;
                sha256_bytes(&data)
            } else {
                "deleted".to_owned()
            };

            self.append_event(
                session_id,
                EventKind::FileMutated {
                    path: path.to_owned(),
                    content_hash,
                },
            )
            .await?;
            emitted += 1;
        }

        let run_dir = PathBuf::from(&manifest.workspace_root)
            .join("tools")
            .join("runs")
            .join(format!("{}", report.tool_run_id.0.hyphenated()));

        fs::create_dir_all(&run_dir).await?;
        self.write_pretty_json(run_dir.join("report.json"), report)
            .await?;

        let observation = extract_observation(&EventRecord::new(
            session_id,
            self.peek_last_sequence(session_id)?,
            EventKind::ToolCallCompleted {
                tool_run_id: report.tool_run_id,
                exit_status: report.exit_status,
                outcome: report.outcome.clone(),
            },
        ));

        if let Some(observation) = observation {
            self.memory
                .append_observation(session_id, &observation)
                .await
                .context("failed appending auto observation")?;
            self.append_event(
                session_id,
                EventKind::ObservationExtracted {
                    observation_id: observation.observation_id,
                },
            )
            .await?;
            emitted += 1;
        }

        Ok(emitted)
    }

    async fn emit_phase(&self, session_id: SessionId, phase: LoopPhase) -> Result<u64> {
        self.append_event(session_id, EventKind::PhaseEntered { phase })
            .await?;
        Ok(1)
    }

    async fn append_event(&self, session_id: SessionId, kind: EventKind) -> Result<()> {
        let sequence = self.next_sequence(session_id)?;
        let event = EventRecord::new(session_id, sequence, kind);
        self.journal.append_and_publish(event).await
    }

    fn next_sequence(&self, session_id: SessionId) -> Result<u64> {
        let mut sessions = self.sessions.lock();
        let session = sessions
            .get_mut(&session_id)
            .with_context(|| format!("session not found: {}", session_id.0))?;
        let sequence = session.next_sequence;
        session.next_sequence += 1;
        Ok(sequence)
    }

    fn peek_last_sequence(&self, session_id: SessionId) -> Result<u64> {
        let sessions = self.sessions.lock();
        let session = sessions
            .get(&session_id)
            .with_context(|| format!("session not found: {}", session_id.0))?;
        Ok(session.next_sequence.saturating_sub(1))
    }

    fn should_checkpoint(&self, session_id: SessionId) -> Result<bool> {
        let mut sessions = self.sessions.lock();
        let session = sessions
            .get_mut(&session_id)
            .with_context(|| format!("session not found: {}", session_id.0))?;
        session.tick_count += 1;
        Ok(session.tick_count % self.config.checkpoint_every_ticks == 0)
    }

    async fn create_checkpoint(
        &self,
        session_id: SessionId,
        manifest: &SessionManifest,
        state: &AgentStateVector,
    ) -> Result<CheckpointManifest> {
        let checkpoint_id = CheckpointId::new();
        let state_hash = sha256_json(state)?;
        let checkpoint = CheckpointManifest {
            checkpoint_id,
            session_id,
            created_at: Utc::now(),
            event_sequence: self.peek_last_sequence(session_id)?,
            state_hash,
            note: "automatic heartbeat checkpoint".to_owned(),
        };

        let checkpoint_dir = PathBuf::from(&manifest.workspace_root)
            .join("checkpoints")
            .join(checkpoint_id.0.hyphenated().to_string());
        fs::create_dir_all(&checkpoint_dir).await?;
        self.write_pretty_json(checkpoint_dir.join("manifest.json"), &checkpoint)
            .await?;
        Ok(checkpoint)
    }

    async fn write_heartbeat(
        &self,
        session_id: SessionId,
        state: &AgentStateVector,
        mode: &OperatingMode,
    ) -> Result<()> {
        let workspace_root = {
            let sessions = self.sessions.lock();
            let session = sessions
                .get(&session_id)
                .with_context(|| format!("session not found: {}", session_id.0))?;
            session.manifest.workspace_root.clone()
        };

        let payload = serde_json::json!({
            "at": Utc::now(),
            "mode": mode,
            "state": state,
        });
        self.write_pretty_json(
            PathBuf::from(workspace_root).join("state/heartbeat.json"),
            &payload,
        )
        .await
    }

    fn persist_runtime_state(
        &self,
        session_id: SessionId,
        state: AgentStateVector,
        mode: OperatingMode,
    ) -> Result<()> {
        let mut sessions = self.sessions.lock();
        let session = sessions
            .get_mut(&session_id)
            .with_context(|| format!("session not found: {}", session_id.0))?;
        session.state_vector = state;
        session.mode = mode;
        Ok(())
    }

    async fn current_tick_output(
        &self,
        session_id: SessionId,
        mode: OperatingMode,
        state: AgentStateVector,
        events_emitted: u64,
    ) -> Result<TickOutput> {
        Ok(TickOutput {
            session_id,
            mode,
            state,
            events_emitted,
            last_sequence: self.peek_last_sequence(session_id)?,
        })
    }

    async fn initialize_workspace(&self, root: &Path) -> Result<()> {
        let directories = [
            "events",
            "checkpoints",
            "state",
            "tools/runs",
            "artifacts/build",
            "artifacts/reports",
            "memory",
            "inbox/human_requests",
            "outbox/ui_stream",
        ];

        for directory in directories {
            fs::create_dir_all(root.join(directory)).await?;
        }

        let thread_path = root.join("state/thread.md");
        if !fs::try_exists(&thread_path).await.unwrap_or(false) {
            fs::write(&thread_path, "# Session Thread\n\n- Session created\n").await?;
        }

        let plan_path = root.join("state/plan.yaml");
        if !fs::try_exists(&plan_path).await.unwrap_or(false) {
            fs::write(
                &plan_path,
                "version: 1\nmode: explore\nsteps:\n  - id: bootstrap\n    status: pending\n",
            )
            .await?;
        }

        let task_graph_path = root.join("state/task_graph.json");
        if !fs::try_exists(&task_graph_path).await.unwrap_or(false) {
            fs::write(
                &task_graph_path,
                serde_json::to_string_pretty(&serde_json::json!({
                    "nodes": [{"id": "bootstrap", "type": "task"}],
                    "edges": [],
                }))?,
            )
            .await?;
        }

        Ok(())
    }

    fn session_root(&self, session_id: SessionId) -> PathBuf {
        self.config
            .root
            .join("sessions")
            .join(session_id.0.hyphenated().to_string())
    }

    async fn write_pretty_json<T: Serialize>(&self, path: PathBuf, value: &T) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        let payload = serde_json::to_string_pretty(value)?;
        fs::write(path, payload).await?;
        Ok(())
    }
}

fn sha256_json<T: Serialize>(value: &T) -> Result<String> {
    let payload = serde_json::to_vec(value)?;
    Ok(sha256_bytes(&payload))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    hex::encode(digest)
}

#[allow(dead_code)]
fn _budget_sanity(budget: &BudgetState) -> Result<()> {
    if budget.cost_remaining_usd < 0.0 {
        bail!("budget cannot be negative");
    }
    Ok(())
}
