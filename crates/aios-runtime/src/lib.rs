use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use aios_events::EventJournal;
use aios_memory::{MemoryStore, extract_observation};
use aios_policy::{ApprovalQueue, SessionPolicyEngine};
use aios_protocol::{
    AgentStateVector, ApprovalId, BranchId, BranchInfo, BranchMergeResult, BudgetState,
    CheckpointId, CheckpointManifest, EventKind, EventRecord, LoopPhase, ModelRouting,
    OperatingMode, PolicySet, RiskLevel, SessionId, SessionManifest, SpanStatus, ToolCall,
    ToolOutcome,
};
use aios_tools::{DispatchResult, ToolContext, ToolDispatcher, ToolExecutionReport};
use anyhow::{Context, Result, bail};
use chrono::Utc;
use parking_lot::Mutex;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::fs;
use tracing::{debug, info, instrument, warn};

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
    next_sequence_by_branch: HashMap<BranchId, u64>,
    branches: HashMap<BranchId, BranchRuntimeState>,
    tick_count: u64,
    mode: OperatingMode,
    state_vector: AgentStateVector,
}

#[derive(Debug, Clone)]
struct BranchRuntimeState {
    parent_branch: Option<BranchId>,
    fork_sequence: u64,
    head_sequence: u64,
    merged_into: Option<BranchId>,
}

#[derive(Clone)]
pub struct KernelRuntime {
    config: RuntimeConfig,
    journal: EventJournal,
    dispatcher: ToolDispatcher,
    memory: Arc<dyn MemoryStore>,
    approvals: ApprovalQueue,
    session_policy: Arc<SessionPolicyEngine>,
    sessions: Arc<Mutex<HashMap<String, SessionRuntimeState>>>,
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

    #[instrument(skip(self, owner, policy, model_routing))]
    pub async fn create_session(
        &self,
        owner: impl Into<String>,
        policy: PolicySet,
        model_routing: ModelRouting,
    ) -> Result<SessionManifest> {
        let session_id = SessionId::default();
        let owner = owner.into();
        let session_root = self.session_root(&session_id);
        self.initialize_workspace(session_root.as_path()).await?;

        let manifest = SessionManifest {
            session_id: session_id.clone(),
            owner,
            created_at: Utc::now(),
            workspace_root: session_root.to_string_lossy().into_owned(),
            model_routing,
            policy: serde_json::to_value(&policy).unwrap_or_default(),
        };

        self.write_pretty_json(session_root.join("manifest.json"), &manifest)
            .await?;

        let manifest_hash = sha256_json(&manifest)?;

        let main_branch = BranchId::main();
        let latest_sequence = self
            .journal
            .latest_sequence(session_id.clone(), Some(main_branch.clone()))
            .await
            .unwrap_or(0);
        let mut next_sequence_by_branch = HashMap::new();
        next_sequence_by_branch.insert(main_branch.clone(), latest_sequence + 1);
        let mut branches = HashMap::new();
        branches.insert(
            main_branch.clone(),
            BranchRuntimeState {
                parent_branch: None,
                fork_sequence: 0,
                head_sequence: latest_sequence,
                merged_into: None,
            },
        );
        self.sessions.lock().insert(
            session_id.as_str().to_owned(),
            SessionRuntimeState {
                manifest: manifest.clone(),
                next_sequence_by_branch,
                branches,
                tick_count: 0,
                mode: OperatingMode::Explore,
                state_vector: AgentStateVector::default(),
            },
        );
        self.session_policy.set_policy(&session_id, &policy).await;

        self.append_event(
            &session_id,
            &main_branch,
            EventKind::SessionCreated {
                name: manifest_hash.clone(),
                config: serde_json::json!({ "manifest_hash": manifest_hash }),
            },
        )
        .await?;

        self.emit_phase(&session_id, &main_branch, LoopPhase::Sleep)
            .await?;

        info!(
            session_id = %session_id,
            workspace_root = %manifest.workspace_root,
            "session created"
        );

        Ok(manifest)
    }

    pub async fn tick(&self, session_id: &SessionId, input: TickInput) -> Result<TickOutput> {
        self.tick_on_branch(session_id, &BranchId::main(), input)
            .await
    }

    #[instrument(
        skip(self, input),
        fields(
            session_id = %session_id,
            branch = %branch_id.as_str(),
            objective_len = input.objective.len(),
            has_tool = input.proposed_tool.is_some()
        )
    )]
    pub async fn tick_on_branch(
        &self,
        session_id: &SessionId,
        branch_id: &BranchId,
        input: TickInput,
    ) -> Result<TickOutput> {
        let (manifest, mut state) = {
            let sessions = self.sessions.lock();
            let session = sessions
                .get(session_id.as_str())
                .with_context(|| format!("session not found: {session_id}"))?;
            (session.manifest.clone(), session.state_vector.clone())
        };

        let mut emitted = 0_u64;

        emitted += self
            .emit_phase(session_id, branch_id, LoopPhase::Perceive)
            .await?;
        emitted += self
            .emit_phase(session_id, branch_id, LoopPhase::Deliberate)
            .await?;

        self.append_event(
            session_id,
            branch_id,
            EventKind::DeliberationProposed {
                summary: input.objective.clone(),
                proposed_tool: input.proposed_tool.as_ref().map(|c| c.tool_name.clone()),
            },
        )
        .await?;
        emitted += 1;

        let pending_approvals = self.approvals.pending_for_session(session_id).await;
        let mut mode = self.estimate_mode(&state, pending_approvals.len());

        self.append_event(
            session_id,
            branch_id,
            EventKind::StateEstimated {
                state: state.clone(),
                mode,
            },
        )
        .await?;
        emitted += 1;
        debug!(mode = ?mode, uncertainty = state.uncertainty, "state estimated");

        if matches!(mode, OperatingMode::AskHuman | OperatingMode::Sleep) {
            emitted += self
                .finalize_tick(session_id, branch_id, &manifest, &mut state, &mode)
                .await?;
            return self
                .current_tick_output(session_id, branch_id, mode, state, emitted)
                .await;
        }

        if let Some(call) = input.proposed_tool {
            emitted += self
                .emit_phase(session_id, branch_id, LoopPhase::Gate)
                .await?;
            self.append_event(
                session_id,
                branch_id,
                EventKind::ToolCallRequested {
                    call_id: call.call_id.clone(),
                    tool_name: call.tool_name.clone(),
                    arguments: call.input.clone(),
                    category: None,
                },
            )
            .await?;
            emitted += 1;

            let context = ToolContext {
                workspace_root: PathBuf::from(&manifest.workspace_root),
            };

            match self
                .dispatcher
                .dispatch(session_id.clone(), &context, call.clone())
                .await
            {
                Ok(DispatchResult::NeedsApproval { evaluation, .. }) => {
                    mode = OperatingMode::AskHuman;
                    info!(
                        approval_count = evaluation.requires_approval.len(),
                        "tool execution requires approval"
                    );
                    for capability in evaluation.requires_approval {
                        let ticket = self
                            .approvals
                            .enqueue(
                                session_id.clone(),
                                capability.clone(),
                                format!("approval required for tool {}", call.tool_name),
                            )
                            .await;
                        self.append_event(
                            session_id,
                            branch_id,
                            EventKind::ApprovalRequested {
                                approval_id: ApprovalId::from_string(
                                    ticket.approval_id.to_string(),
                                ),
                                call_id: call.call_id.clone(),
                                tool_name: call.tool_name.clone(),
                                arguments: call.input.clone(),
                                risk: RiskLevel::Medium,
                            },
                        )
                        .await?;
                        emitted += 1;
                    }
                }
                Ok(DispatchResult::Executed(report)) => {
                    emitted += self
                        .emit_phase(session_id, branch_id, LoopPhase::Execute)
                        .await?;
                    emitted += self
                        .record_tool_report(session_id, branch_id, &manifest, &report)
                        .await?;
                    self.apply_homeostasis_controllers(&mut state, &report);
                    mode = self.estimate_mode(&state, 0);
                    info!(
                        tool_name = %report.tool_name,
                        tool_run_id = %report.tool_run_id,
                        exit_status = report.exit_status,
                        mode = ?mode,
                        "tool execution completed"
                    );
                }
                Err(error) => {
                    state.error_streak += 1;
                    state.uncertainty = (state.uncertainty + 0.15).min(1.0);
                    state.budget.error_budget_remaining =
                        state.budget.error_budget_remaining.saturating_sub(1);
                    mode = OperatingMode::Recover;
                    warn!(error = %error, error_streak = state.error_streak, "tool execution failed");

                    self.append_event(
                        session_id,
                        branch_id,
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
            warn!(
                error_streak = state.error_streak,
                threshold = self.config.circuit_breaker_errors,
                "circuit breaker tripped"
            );
            self.append_event(
                session_id,
                branch_id,
                EventKind::CircuitBreakerTripped {
                    reason: "error streak exceeded threshold".to_owned(),
                    error_streak: state.error_streak,
                },
            )
            .await?;
            emitted += 1;
        }

        emitted += self
            .finalize_tick(session_id, branch_id, &manifest, &mut state, &mode)
            .await?;
        info!(mode = ?mode, emitted, "tick finalized");
        self.current_tick_output(session_id, branch_id, mode, state, emitted)
            .await
    }

    #[instrument(
        skip(self),
        fields(
            session_id = %session_id,
            branch = %branch_id.as_str(),
            from_branch = ?from_branch.as_ref().map(|branch| branch.as_str())
        )
    )]
    pub async fn create_branch(
        &self,
        session_id: &SessionId,
        branch_id: BranchId,
        from_branch: Option<BranchId>,
        fork_sequence: Option<u64>,
    ) -> Result<BranchInfo> {
        let from_branch = from_branch.unwrap_or_else(BranchId::main);
        let fork_sequence_value = {
            let mut sessions = self.sessions.lock();
            let session = sessions
                .get_mut(session_id.as_str())
                .with_context(|| format!("session not found: {session_id}"))?;
            if session.branches.contains_key(&branch_id) {
                bail!("branch already exists: {}", branch_id.as_str());
            }
            let parent = session
                .branches
                .get(&from_branch)
                .with_context(|| format!("source branch not found: {}", from_branch.as_str()))?;
            if let Some(target) = &parent.merged_into {
                bail!(
                    "source branch {} is merged into {} and is read-only",
                    from_branch.as_str(),
                    target.as_str()
                );
            }
            let fork = fork_sequence.unwrap_or(parent.head_sequence);
            if fork > parent.head_sequence {
                bail!(
                    "fork sequence {} exceeds source branch head {}",
                    fork,
                    parent.head_sequence
                );
            }

            session.next_sequence_by_branch.insert(branch_id.clone(), 1);
            session.branches.insert(
                branch_id.clone(),
                BranchRuntimeState {
                    parent_branch: Some(from_branch.clone()),
                    fork_sequence: fork,
                    head_sequence: 0,
                    merged_into: None,
                },
            );
            fork
        };

        self.append_event(
            session_id,
            &branch_id,
            EventKind::BranchCreated {
                new_branch_id: branch_id.clone(),
                fork_point_seq: fork_sequence_value,
                name: branch_id.as_str().to_owned(),
            },
        )
        .await?;
        info!(
            branch = %branch_id.as_str(),
            from_branch = %from_branch.as_str(),
            fork_sequence = fork_sequence_value,
            "branch created"
        );

        self.branch_info(session_id, &branch_id)
    }

    pub async fn list_branches(&self, session_id: &SessionId) -> Result<Vec<BranchInfo>> {
        let sessions = self.sessions.lock();
        let session = sessions
            .get(session_id.as_str())
            .with_context(|| format!("session not found: {session_id}"))?;

        let mut branches: Vec<_> = session
            .branches
            .iter()
            .map(|(branch_id, state)| BranchInfo {
                branch_id: branch_id.clone(),
                parent_branch: state.parent_branch.clone(),
                fork_sequence: state.fork_sequence,
                head_sequence: state.head_sequence,
                merged_into: state.merged_into.clone(),
            })
            .collect();
        branches.sort_by(|a, b| a.branch_id.as_str().cmp(b.branch_id.as_str()));
        Ok(branches)
    }

    #[instrument(
        skip(self),
        fields(
            session_id = %session_id,
            source_branch = %source_branch.as_str(),
            target_branch = %target_branch.as_str()
        )
    )]
    pub async fn merge_branch(
        &self,
        session_id: &SessionId,
        source_branch: BranchId,
        target_branch: BranchId,
    ) -> Result<BranchMergeResult> {
        if source_branch == target_branch {
            bail!("source and target branch must differ");
        }
        if source_branch == BranchId::main() {
            bail!("main branch cannot be used as a merge source");
        }

        let source_head =
            {
                let sessions = self.sessions.lock();
                let session = sessions
                    .get(session_id.as_str())
                    .with_context(|| format!("session not found: {session_id}"))?;
                let source = session.branches.get(&source_branch).with_context(|| {
                    format!("source branch not found: {}", source_branch.as_str())
                })?;
                if let Some(merged_into) = &source.merged_into {
                    bail!(
                        "source branch {} already merged into {}",
                        source_branch.as_str(),
                        merged_into.as_str()
                    );
                }
                let target = session.branches.get(&target_branch).with_context(|| {
                    format!("target branch not found: {}", target_branch.as_str())
                })?;
                if let Some(merged_into) = &target.merged_into {
                    bail!(
                        "target branch {} is merged into {} and is read-only",
                        target_branch.as_str(),
                        merged_into.as_str()
                    );
                }
                source.head_sequence
            };

        self.append_event(
            session_id,
            &target_branch,
            EventKind::BranchMerged {
                source_branch_id: source_branch.clone(),
                merge_seq: source_head,
            },
        )
        .await?;

        let target_head = self.peek_last_sequence(session_id, &target_branch)?;
        {
            let mut sessions = self.sessions.lock();
            let session = sessions
                .get_mut(session_id.as_str())
                .with_context(|| format!("session not found: {session_id}"))?;
            let source = session
                .branches
                .get_mut(&source_branch)
                .with_context(|| format!("source branch not found: {}", source_branch.as_str()))?;
            source.merged_into = Some(target_branch.clone());
        }
        info!(
            source_head_sequence = source_head,
            target_head_sequence = target_head,
            "branch merged"
        );

        Ok(BranchMergeResult {
            source_branch,
            target_branch,
            source_head_sequence: source_head,
            target_head_sequence: target_head,
        })
    }

    pub async fn resolve_approval(
        &self,
        session_id: &SessionId,
        approval_id: uuid::Uuid,
        approved: bool,
        actor: impl Into<String>,
    ) -> Result<()> {
        let actor = actor.into();
        let resolution = self
            .approvals
            .resolve(approval_id, approved, actor.clone())
            .await
            .with_context(|| format!("approval not pending: {approval_id}"))?;

        let decision = if resolution.approved {
            aios_protocol::ApprovalDecision::Approved
        } else {
            aios_protocol::ApprovalDecision::Denied
        };

        self.append_event(
            session_id,
            &BranchId::main(),
            EventKind::ApprovalResolved {
                approval_id: ApprovalId::from_string(approval_id.to_string()),
                decision,
                reason: Some(actor),
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
        session_id: &SessionId,
        kind: EventKind,
    ) -> Result<()> {
        self.record_external_event_on_branch(session_id, &BranchId::main(), kind)
            .await
    }

    #[instrument(
        skip(self, kind),
        fields(session_id = %session_id, branch = %branch_id.as_str())
    )]
    pub async fn record_external_event_on_branch(
        &self,
        session_id: &SessionId,
        branch_id: &BranchId,
        kind: EventKind,
    ) -> Result<()> {
        {
            let sessions = self.sessions.lock();
            if !sessions.contains_key(session_id.as_str()) {
                bail!("session not found: {session_id}");
            }
        }
        self.append_event(session_id, branch_id, kind).await
    }

    pub async fn read_events(
        &self,
        session_id: &SessionId,
        from_sequence: u64,
        limit: usize,
    ) -> Result<Vec<EventRecord>> {
        self.read_events_on_branch(session_id, &BranchId::main(), from_sequence, limit)
            .await
    }

    pub async fn read_events_on_branch(
        &self,
        session_id: &SessionId,
        branch_id: &BranchId,
        from_sequence: u64,
        limit: usize,
    ) -> Result<Vec<EventRecord>> {
        self.journal
            .read_from(
                session_id.clone(),
                Some(branch_id.clone()),
                from_sequence,
                limit,
            )
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
        session_id: &SessionId,
        branch_id: &BranchId,
        manifest: &SessionManifest,
        state: &mut AgentStateVector,
        mode: &OperatingMode,
    ) -> Result<u64> {
        let mut emitted = 0_u64;

        emitted += self
            .emit_phase(session_id, branch_id, LoopPhase::Reflect)
            .await?;

        self.append_event(
            session_id,
            branch_id,
            EventKind::BudgetUpdated {
                budget: state.budget.clone(),
                reason: "tick accounting".to_owned(),
            },
        )
        .await?;
        emitted += 1;

        self.append_event(
            session_id,
            branch_id,
            EventKind::StateEstimated {
                state: state.clone(),
                mode: *mode,
            },
        )
        .await?;
        emitted += 1;

        let checkpoint_id = if self.should_checkpoint(session_id)? {
            let checkpoint = self
                .create_checkpoint(session_id, branch_id, manifest, state)
                .await?;
            self.append_event(
                session_id,
                branch_id,
                EventKind::CheckpointCreated {
                    checkpoint_id: checkpoint.checkpoint_id.clone(),
                    event_sequence: checkpoint.event_sequence,
                    state_hash: checkpoint.state_hash.clone(),
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
            branch_id,
            EventKind::Heartbeat {
                summary: "tick complete".to_owned(),
                checkpoint_id,
            },
        )
        .await?;
        emitted += 1;

        emitted += self
            .emit_phase(session_id, branch_id, LoopPhase::Sleep)
            .await?;

        self.persist_runtime_state(session_id, state.clone(), *mode)?;

        Ok(emitted)
    }

    async fn record_tool_report(
        &self,
        session_id: &SessionId,
        branch_id: &BranchId,
        manifest: &SessionManifest,
        report: &ToolExecutionReport,
    ) -> Result<u64> {
        let mut emitted = 0;

        self.append_event(
            session_id,
            branch_id,
            EventKind::ToolCallStarted {
                tool_run_id: report.tool_run_id.clone(),
                tool_name: report.tool_name.clone(),
            },
        )
        .await?;
        emitted += 1;

        let status = if report.exit_status == 0 {
            SpanStatus::Ok
        } else {
            SpanStatus::Error
        };
        let result_value = serde_json::to_value(&report.outcome).unwrap_or_default();

        self.append_event(
            session_id,
            branch_id,
            EventKind::ToolCallCompleted {
                tool_run_id: report.tool_run_id.clone(),
                call_id: None,
                tool_name: report.tool_name.clone(),
                result: result_value,
                duration_ms: 0,
                status,
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
                branch_id,
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
            .join(report.tool_run_id.as_str());

        fs::create_dir_all(&run_dir).await?;
        self.write_pretty_json(run_dir.join("report.json"), report)
            .await?;

        let observation = extract_observation(&EventRecord::new(
            session_id.clone(),
            branch_id.clone(),
            self.peek_last_sequence(session_id, branch_id)?,
            EventKind::ToolCallCompleted {
                tool_run_id: report.tool_run_id.clone(),
                call_id: None,
                tool_name: report.tool_name.clone(),
                result: serde_json::to_value(&report.outcome).unwrap_or_default(),
                duration_ms: 0,
                status,
            },
        ));

        if let Some(observation) = observation {
            self.memory
                .append_observation(session_id, &observation)
                .await
                .context("failed appending auto observation")?;
            self.append_event(
                session_id,
                branch_id,
                EventKind::Custom {
                    event_type: "ObservationExtracted".to_owned(),
                    data: serde_json::json!({
                        "observation_id": observation.observation_id.to_string(),
                    }),
                },
            )
            .await?;
            emitted += 1;
        }

        Ok(emitted)
    }

    async fn emit_phase(
        &self,
        session_id: &SessionId,
        branch_id: &BranchId,
        phase: LoopPhase,
    ) -> Result<u64> {
        self.append_event(session_id, branch_id, EventKind::PhaseEntered { phase })
            .await?;
        Ok(1)
    }

    async fn append_event(
        &self,
        session_id: &SessionId,
        branch_id: &BranchId,
        kind: EventKind,
    ) -> Result<()> {
        let event_kind = event_kind_name(&kind);
        let sequence = self.next_sequence(session_id, branch_id)?;
        debug!(
            session_id = %session_id,
            branch = %branch_id.as_str(),
            sequence,
            event_kind,
            "appending event"
        );
        let event = EventRecord::new(session_id.clone(), branch_id.clone(), sequence, kind);
        if let Err(append_error) = self.journal.append_and_publish(event).await {
            if let Err(resync_error) = self.resync_next_sequence(session_id, branch_id).await {
                warn!(
                    session_id = %session_id,
                    branch = %branch_id.as_str(),
                    error = %append_error,
                    resync_error = %resync_error,
                    "event append failed and sequence resync failed"
                );
                return Err(append_error).context(format!(
                    "failed appending event and failed sequence resync: {resync_error}"
                ));
            }
            warn!(
                session_id = %session_id,
                branch = %branch_id.as_str(),
                error = %append_error,
                "event append failed; sequence resynced"
            );
            return Err(append_error).context("failed appending event; sequence was resynced");
        }
        self.mark_branch_head(session_id, branch_id, sequence)?;
        Ok(())
    }

    fn next_sequence(&self, session_id: &SessionId, branch_id: &BranchId) -> Result<u64> {
        let mut sessions = self.sessions.lock();
        let session = sessions
            .get_mut(session_id.as_str())
            .with_context(|| format!("session not found: {session_id}"))?;
        if !session.branches.contains_key(branch_id) {
            bail!("branch not found: {}", branch_id.as_str());
        }
        if let Some(merged_into) = session
            .branches
            .get(branch_id)
            .and_then(|branch| branch.merged_into.as_ref())
        {
            bail!(
                "branch {} is merged into {} and is read-only",
                branch_id.as_str(),
                merged_into.as_str()
            );
        }
        let sequence = *session
            .next_sequence_by_branch
            .entry(branch_id.clone())
            .or_insert(1);
        session
            .next_sequence_by_branch
            .insert(branch_id.clone(), sequence.saturating_add(1));
        Ok(sequence)
    }

    fn peek_last_sequence(&self, session_id: &SessionId, branch_id: &BranchId) -> Result<u64> {
        let sessions = self.sessions.lock();
        let session = sessions
            .get(session_id.as_str())
            .with_context(|| format!("session not found: {session_id}"))?;
        if !session.branches.contains_key(branch_id) {
            bail!("branch not found: {}", branch_id.as_str());
        }
        Ok(session
            .next_sequence_by_branch
            .get(branch_id)
            .copied()
            .unwrap_or(1)
            .saturating_sub(1))
    }

    async fn resync_next_sequence(
        &self,
        session_id: &SessionId,
        branch_id: &BranchId,
    ) -> Result<()> {
        let latest = self
            .journal
            .latest_sequence(session_id.clone(), Some(branch_id.clone()))
            .await
            .context("failed loading latest sequence for resync")?;
        let mut sessions = self.sessions.lock();
        let session = sessions
            .get_mut(session_id.as_str())
            .with_context(|| format!("session not found: {session_id}"))?;
        if !session.branches.contains_key(branch_id) {
            bail!("branch not found: {}", branch_id.as_str());
        }
        session
            .next_sequence_by_branch
            .insert(branch_id.clone(), latest.saturating_add(1));
        Ok(())
    }

    fn mark_branch_head(
        &self,
        session_id: &SessionId,
        branch_id: &BranchId,
        sequence: u64,
    ) -> Result<()> {
        let mut sessions = self.sessions.lock();
        let session = sessions
            .get_mut(session_id.as_str())
            .with_context(|| format!("session not found: {session_id}"))?;
        let branch = session
            .branches
            .get_mut(branch_id)
            .with_context(|| format!("branch not found: {}", branch_id.as_str()))?;
        branch.head_sequence = branch.head_sequence.max(sequence);
        Ok(())
    }

    fn branch_info(&self, session_id: &SessionId, branch_id: &BranchId) -> Result<BranchInfo> {
        let sessions = self.sessions.lock();
        let session = sessions
            .get(session_id.as_str())
            .with_context(|| format!("session not found: {session_id}"))?;
        let state = session
            .branches
            .get(branch_id)
            .with_context(|| format!("branch not found: {}", branch_id.as_str()))?;
        Ok(BranchInfo {
            branch_id: branch_id.clone(),
            parent_branch: state.parent_branch.clone(),
            fork_sequence: state.fork_sequence,
            head_sequence: state.head_sequence,
            merged_into: state.merged_into.clone(),
        })
    }

    fn should_checkpoint(&self, session_id: &SessionId) -> Result<bool> {
        let mut sessions = self.sessions.lock();
        let session = sessions
            .get_mut(session_id.as_str())
            .with_context(|| format!("session not found: {session_id}"))?;
        session.tick_count += 1;
        Ok(session.tick_count % self.config.checkpoint_every_ticks == 0)
    }

    async fn create_checkpoint(
        &self,
        session_id: &SessionId,
        branch_id: &BranchId,
        manifest: &SessionManifest,
        state: &AgentStateVector,
    ) -> Result<CheckpointManifest> {
        let checkpoint_id = CheckpointId::default();
        let state_hash = sha256_json(state)?;
        let checkpoint = CheckpointManifest {
            checkpoint_id: checkpoint_id.clone(),
            session_id: session_id.clone(),
            branch_id: branch_id.clone(),
            created_at: Utc::now(),
            event_sequence: self.peek_last_sequence(session_id, branch_id)?,
            state_hash,
            note: "automatic heartbeat checkpoint".to_owned(),
        };

        let checkpoint_dir = PathBuf::from(&manifest.workspace_root)
            .join("checkpoints")
            .join(checkpoint_id.as_str());
        fs::create_dir_all(&checkpoint_dir).await?;
        self.write_pretty_json(checkpoint_dir.join("manifest.json"), &checkpoint)
            .await?;
        Ok(checkpoint)
    }

    async fn write_heartbeat(
        &self,
        session_id: &SessionId,
        state: &AgentStateVector,
        mode: &OperatingMode,
    ) -> Result<()> {
        let workspace_root = {
            let sessions = self.sessions.lock();
            let session = sessions
                .get(session_id.as_str())
                .with_context(|| format!("session not found: {session_id}"))?;
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
        session_id: &SessionId,
        state: AgentStateVector,
        mode: OperatingMode,
    ) -> Result<()> {
        let mut sessions = self.sessions.lock();
        let session = sessions
            .get_mut(session_id.as_str())
            .with_context(|| format!("session not found: {session_id}"))?;
        session.state_vector = state;
        session.mode = mode;
        Ok(())
    }

    async fn current_tick_output(
        &self,
        session_id: &SessionId,
        branch_id: &BranchId,
        mode: OperatingMode,
        state: AgentStateVector,
        events_emitted: u64,
    ) -> Result<TickOutput> {
        Ok(TickOutput {
            session_id: session_id.clone(),
            mode,
            state,
            events_emitted,
            last_sequence: self.peek_last_sequence(session_id, branch_id)?,
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

    fn session_root(&self, session_id: &SessionId) -> PathBuf {
        self.config.root.join("sessions").join(session_id.as_str())
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

fn event_kind_name(kind: &EventKind) -> &'static str {
    match kind {
        EventKind::SessionCreated { .. } => "session_created",
        EventKind::BranchCreated { .. } => "branch_created",
        EventKind::BranchMerged { .. } => "branch_merged",
        EventKind::PhaseEntered { .. } => "phase_entered",
        EventKind::DeliberationProposed { .. } => "deliberation_proposed",
        EventKind::ApprovalRequested { .. } => "approval_requested",
        EventKind::ApprovalResolved { .. } => "approval_resolved",
        EventKind::ToolCallRequested { .. } => "tool_call_requested",
        EventKind::ToolCallStarted { .. } => "tool_call_started",
        EventKind::ToolCallCompleted { .. } => "tool_call_completed",
        EventKind::VoiceSessionStarted { .. } => "voice_session_started",
        EventKind::VoiceInputChunk { .. } => "voice_input_chunk",
        EventKind::VoiceOutputChunk { .. } => "voice_output_chunk",
        EventKind::VoiceSessionStopped { .. } => "voice_session_stopped",
        EventKind::VoiceAdapterError { .. } => "voice_adapter_error",
        EventKind::FileMutated { .. } => "file_mutated",
        EventKind::Heartbeat { .. } => "heartbeat",
        EventKind::CheckpointCreated { .. } => "checkpoint_created",
        EventKind::StateEstimated { .. } => "state_estimated",
        EventKind::BudgetUpdated { .. } => "budget_updated",
        EventKind::CircuitBreakerTripped { .. } => "circuit_breaker_tripped",
        EventKind::ErrorRaised { .. } => "error_raised",
        _ => "custom",
    }
}

#[allow(dead_code)]
fn _budget_sanity(budget: &BudgetState) -> Result<()> {
    if budget.cost_remaining_usd < 0.0 {
        bail!("budget cannot be negative");
    }
    Ok(())
}
