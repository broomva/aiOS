use std::path::PathBuf;
use std::sync::Arc;

use aios_events::{EventJournal, EventStreamHub, FileEventStore};
use aios_memory::WorkspaceMemoryStore;
use aios_model::{
    BranchId, BranchInfo, BranchMergeResult, EventKind, EventRecord, ModelRouting, PolicySet,
    SessionId, SessionManifest, ToolCall,
};
use aios_policy::{ApprovalQueue, SessionPolicyEngine};
use aios_runtime::{KernelRuntime, RuntimeConfig, TickInput, TickOutput};
use aios_sandbox::LocalSandboxRunner;
use aios_tools::{ToolDispatcher, ToolRegistry};
use anyhow::Result;
use tracing::instrument;

#[derive(Debug, Clone)]
pub struct KernelBuilder {
    root: PathBuf,
    allowed_commands: Vec<String>,
    default_policy: PolicySet,
}

impl KernelBuilder {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            allowed_commands: vec!["echo".to_owned(), "git".to_owned(), "cargo".to_owned()],
            default_policy: PolicySet::default(),
        }
    }

    pub fn allowed_commands(mut self, allowed_commands: Vec<String>) -> Self {
        self.allowed_commands = allowed_commands;
        self
    }

    pub fn default_policy(mut self, policy: PolicySet) -> Self {
        self.default_policy = policy;
        self
    }

    pub fn build(self) -> AiosKernel {
        let events_root = self.root.join("kernel");
        let session_root = self.root.join("sessions");

        let event_store = Arc::new(FileEventStore::new(events_root));
        let stream = EventStreamHub::new(1024);
        let journal = EventJournal::new(event_store, stream);

        let approvals = ApprovalQueue::default();
        let policy_engine = Arc::new(SessionPolicyEngine::new(self.default_policy));

        let registry = Arc::new(ToolRegistry::with_core_tools());
        let sandbox = Arc::new(LocalSandboxRunner::new(self.allowed_commands));
        let dispatcher = ToolDispatcher::new(registry, policy_engine.clone(), sandbox);

        let memory = Arc::new(WorkspaceMemoryStore::new(session_root));
        let runtime = KernelRuntime::new(
            RuntimeConfig::new(self.root),
            journal,
            dispatcher,
            memory,
            approvals,
            policy_engine,
        );

        AiosKernel { runtime }
    }
}

#[derive(Clone)]
pub struct AiosKernel {
    runtime: KernelRuntime,
}

impl AiosKernel {
    #[instrument(skip(self, owner, policy, model_routing))]
    pub async fn create_session(
        &self,
        owner: impl Into<String>,
        policy: PolicySet,
        model_routing: Option<ModelRouting>,
    ) -> Result<SessionManifest> {
        self.runtime
            .create_session(owner, policy, model_routing.unwrap_or_default())
            .await
    }

    #[instrument(skip(self, objective, proposed_tool), fields(session_id = %session_id.0))]
    pub async fn tick(
        &self,
        session_id: SessionId,
        objective: impl Into<String>,
        proposed_tool: Option<ToolCall>,
    ) -> Result<TickOutput> {
        self.tick_on_branch(session_id, BranchId::main(), objective, proposed_tool)
            .await
    }

    #[instrument(
        skip(self, objective, proposed_tool),
        fields(session_id = %session_id.0, branch = %branch_id.as_str())
    )]
    pub async fn tick_on_branch(
        &self,
        session_id: SessionId,
        branch_id: BranchId,
        objective: impl Into<String>,
        proposed_tool: Option<ToolCall>,
    ) -> Result<TickOutput> {
        self.runtime
            .tick_on_branch(
                session_id,
                branch_id,
                TickInput {
                    objective: objective.into(),
                    proposed_tool,
                },
            )
            .await
    }

    #[instrument(
        skip(self),
        fields(
            session_id = %session_id.0,
            branch = %branch_id.as_str(),
            from_branch = ?from_branch.as_ref().map(|branch| branch.as_str())
        )
    )]
    pub async fn create_branch(
        &self,
        session_id: SessionId,
        branch_id: BranchId,
        from_branch: Option<BranchId>,
        fork_sequence: Option<u64>,
    ) -> Result<BranchInfo> {
        self.runtime
            .create_branch(session_id, branch_id, from_branch, fork_sequence)
            .await
    }

    #[instrument(skip(self), fields(session_id = %session_id.0))]
    pub async fn list_branches(&self, session_id: SessionId) -> Result<Vec<BranchInfo>> {
        self.runtime.list_branches(session_id).await
    }

    #[instrument(
        skip(self),
        fields(
            session_id = %session_id.0,
            source_branch = %source_branch.as_str(),
            target_branch = %target_branch.as_str()
        )
    )]
    pub async fn merge_branch(
        &self,
        session_id: SessionId,
        source_branch: BranchId,
        target_branch: BranchId,
    ) -> Result<BranchMergeResult> {
        self.runtime
            .merge_branch(session_id, source_branch, target_branch)
            .await
    }

    pub async fn resolve_approval(
        &self,
        session_id: SessionId,
        approval_id: uuid::Uuid,
        approved: bool,
        actor: impl Into<String>,
    ) -> Result<()> {
        self.runtime
            .resolve_approval(session_id, approval_id, approved, actor)
            .await
    }

    pub fn subscribe_events(&self) -> tokio::sync::broadcast::Receiver<EventRecord> {
        self.runtime.subscribe_events()
    }

    pub async fn record_external_event(
        &self,
        session_id: SessionId,
        kind: EventKind,
    ) -> Result<()> {
        self.record_external_event_on_branch(session_id, BranchId::main(), kind)
            .await
    }

    pub async fn record_external_event_on_branch(
        &self,
        session_id: SessionId,
        branch_id: BranchId,
        kind: EventKind,
    ) -> Result<()> {
        self.runtime
            .record_external_event_on_branch(session_id, branch_id, kind)
            .await
    }

    pub async fn read_events(
        &self,
        session_id: SessionId,
        from_sequence: u64,
        limit: usize,
    ) -> Result<Vec<EventRecord>> {
        self.read_events_on_branch(session_id, BranchId::main(), from_sequence, limit)
            .await
    }

    pub async fn read_events_on_branch(
        &self,
        session_id: SessionId,
        branch_id: BranchId,
        from_sequence: u64,
        limit: usize,
    ) -> Result<Vec<EventRecord>> {
        self.runtime
            .read_events_on_branch(session_id, branch_id, from_sequence, limit)
            .await
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use aios_model::{BranchId, Capability, OperatingMode, PolicySet, ToolCall};
    use anyhow::Result;
    use serde_json::json;
    use tokio::fs;

    use crate::KernelBuilder;

    fn unique_test_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("{name}-{nanos}"))
    }

    #[tokio::test]
    async fn successful_tick_writes_artifact_and_advances_progress() -> Result<()> {
        let root = unique_test_root("aios-kernel-success");
        let kernel = KernelBuilder::new(&root)
            .allowed_commands(vec!["echo".to_owned()])
            .build();

        let policy = PolicySet {
            allow_capabilities: vec![
                Capability::fs_read("/session/**"),
                Capability::fs_write("/session/**"),
                Capability::exec("*"),
            ],
            gate_capabilities: vec![],
            max_tool_runtime_secs: 10,
            max_events_per_turn: 128,
        };

        let session = kernel.create_session("tester", policy, None).await?;
        let call = ToolCall::new(
            "fs.write",
            json!({
                "path": "artifacts/reports/test.txt",
                "content": "ok"
            }),
            vec![Capability::fs_write("/session/artifacts/**")],
        );

        let tick = kernel
            .tick(session.session_id, "write test artifact", Some(call))
            .await?;
        assert!(tick.state.progress > 0.0);
        assert!(matches!(
            tick.mode,
            OperatingMode::Execute | OperatingMode::Explore | OperatingMode::Verify
        ));

        let artifact_path =
            PathBuf::from(&session.workspace_root).join("artifacts/reports/test.txt");
        let content = fs::read_to_string(artifact_path).await?;
        assert_eq!(content, "ok");

        let _ = fs::remove_dir_all(root).await;
        Ok(())
    }

    #[tokio::test]
    async fn denied_tool_call_triggers_recover_mode() -> Result<()> {
        let root = unique_test_root("aios-kernel-recover");
        let kernel = KernelBuilder::new(&root).allowed_commands(vec![]).build();

        let restrictive_policy = PolicySet {
            allow_capabilities: vec![Capability::fs_read("/session/**")],
            gate_capabilities: vec![],
            max_tool_runtime_secs: 10,
            max_events_per_turn: 128,
        };

        let session = kernel
            .create_session("tester", restrictive_policy, None)
            .await?;
        let forbidden = ToolCall::new(
            "shell.exec",
            json!({
                "command": "echo",
                "args": ["blocked"],
            }),
            vec![Capability::exec("echo")],
        );

        let tick = kernel
            .tick(
                session.session_id,
                "attempt forbidden command",
                Some(forbidden),
            )
            .await?;

        assert!(matches!(tick.mode, OperatingMode::Recover));
        assert_eq!(tick.state.error_streak, 1);
        assert_eq!(tick.state.budget.error_budget_remaining, 7);

        let _ = fs::remove_dir_all(root).await;
        Ok(())
    }

    #[tokio::test]
    async fn branch_ticks_keep_independent_sequences() -> Result<()> {
        let root = unique_test_root("aios-kernel-branches");
        let kernel = KernelBuilder::new(&root).build();
        let session = kernel
            .create_session("tester", PolicySet::default(), None)
            .await?;

        let feature = BranchId::new("feature-a");
        let created = kernel
            .create_branch(
                session.session_id,
                feature.clone(),
                Some(BranchId::main()),
                None,
            )
            .await?;
        assert_eq!(created.branch_id, feature);
        let _ = kernel
            .tick_on_branch(session.session_id, BranchId::main(), "main tick", None)
            .await?;
        let _ = kernel
            .tick_on_branch(session.session_id, feature.clone(), "feature tick", None)
            .await?;

        let main_events = kernel
            .read_events_on_branch(session.session_id, BranchId::main(), 1, 256)
            .await?;
        let feature_events = kernel
            .read_events_on_branch(session.session_id, feature.clone(), 1, 256)
            .await?;

        assert!(!main_events.is_empty());
        assert!(!feature_events.is_empty());
        assert!(
            feature_events
                .iter()
                .all(|event| event.branch_id == feature)
        );
        assert_eq!(feature_events[0].sequence, 1);

        let _ = fs::remove_dir_all(root).await;
        Ok(())
    }

    #[tokio::test]
    async fn replay_reads_are_stable_per_branch() -> Result<()> {
        let root = unique_test_root("aios-kernel-replay");
        let kernel = KernelBuilder::new(&root).build();
        let session = kernel
            .create_session("tester", PolicySet::default(), None)
            .await?;

        let feature = BranchId::new("feature-replay");
        kernel
            .create_branch(
                session.session_id,
                feature.clone(),
                Some(BranchId::main()),
                None,
            )
            .await?;

        let _ = kernel
            .tick_on_branch(session.session_id, BranchId::main(), "main tick 1", None)
            .await?;
        let main_snapshot = kernel
            .read_events_on_branch(session.session_id, BranchId::main(), 1, 512)
            .await?;
        let main_snapshot_json = serde_json::to_string(&main_snapshot)?;

        let _ = kernel
            .tick_on_branch(session.session_id, feature.clone(), "feature tick 1", None)
            .await?;

        let main_after_feature = kernel
            .read_events_on_branch(session.session_id, BranchId::main(), 1, 512)
            .await?;
        let main_after_feature_json = serde_json::to_string(&main_after_feature)?;
        assert_eq!(main_snapshot_json, main_after_feature_json);

        let feature_events_first = kernel
            .read_events_on_branch(session.session_id, feature.clone(), 1, 512)
            .await?;
        let feature_events_second = kernel
            .read_events_on_branch(session.session_id, feature.clone(), 1, 512)
            .await?;
        assert_eq!(
            serde_json::to_string(&feature_events_first)?,
            serde_json::to_string(&feature_events_second)?
        );

        let _ = fs::remove_dir_all(root).await;
        Ok(())
    }

    #[tokio::test]
    async fn merged_branch_becomes_read_only() -> Result<()> {
        let root = unique_test_root("aios-kernel-merge-readonly");
        let kernel = KernelBuilder::new(&root).build();
        let session = kernel
            .create_session("tester", PolicySet::default(), None)
            .await?;

        let feature = BranchId::new("feature-merge");
        kernel
            .create_branch(
                session.session_id,
                feature.clone(),
                Some(BranchId::main()),
                None,
            )
            .await?;

        let _ = kernel
            .tick_on_branch(session.session_id, feature.clone(), "feature tick 1", None)
            .await?;

        let merge = kernel
            .merge_branch(session.session_id, feature.clone(), BranchId::main())
            .await?;
        assert_eq!(merge.source_branch, feature);

        let branches = kernel.list_branches(session.session_id).await?;
        let feature_info = branches
            .iter()
            .find(|branch| branch.branch_id == feature)
            .expect("feature branch exists after merge");
        assert_eq!(feature_info.merged_into, Some(BranchId::main()));

        let second_merge_error = kernel
            .merge_branch(session.session_id, feature.clone(), BranchId::main())
            .await
            .expect_err("branch should not merge twice");
        assert!(second_merge_error.to_string().contains("already merged"));

        let tick_error = kernel
            .tick_on_branch(
                session.session_id,
                feature.clone(),
                "tick after merge",
                None,
            )
            .await
            .expect_err("merged branch should be read-only");
        assert!(tick_error.to_string().contains("read-only"));

        let _ = kernel
            .tick_on_branch(
                session.session_id,
                BranchId::main(),
                "main still writable",
                None,
            )
            .await?;

        let _ = fs::remove_dir_all(root).await;
        Ok(())
    }

    #[tokio::test]
    async fn create_branch_rejects_fork_sequence_past_source_head() -> Result<()> {
        let root = unique_test_root("aios-kernel-fork-sequence");
        let kernel = KernelBuilder::new(&root).build();
        let session = kernel
            .create_session("tester", PolicySet::default(), None)
            .await?;

        let feature = BranchId::new("feature-invalid-fork");
        let error = kernel
            .create_branch(
                session.session_id,
                feature,
                Some(BranchId::main()),
                Some(1_000),
            )
            .await
            .expect_err("fork sequence beyond source head should fail");
        assert!(error.to_string().contains("exceeds source branch head"));

        let _ = fs::remove_dir_all(root).await;
        Ok(())
    }

    #[tokio::test]
    async fn merge_rejects_main_as_source_branch() -> Result<()> {
        let root = unique_test_root("aios-kernel-main-merge-source");
        let kernel = KernelBuilder::new(&root).build();
        let session = kernel
            .create_session("tester", PolicySet::default(), None)
            .await?;

        let feature = BranchId::new("feature-main-merge");
        kernel
            .create_branch(
                session.session_id,
                feature.clone(),
                Some(BranchId::main()),
                None,
            )
            .await?;

        let error = kernel
            .merge_branch(session.session_id, BranchId::main(), feature)
            .await
            .expect_err("main should not be a merge source");
        assert!(
            error
                .to_string()
                .contains("main branch cannot be used as a merge source")
        );

        let _ = fs::remove_dir_all(root).await;
        Ok(())
    }
}
