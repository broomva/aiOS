use std::path::PathBuf;
use std::sync::Arc;

use aios_events::{EventJournal, EventStreamHub, FileEventStore};
use aios_memory::WorkspaceMemoryStore;
use aios_model::{EventRecord, ModelRouting, PolicySet, SessionId, SessionManifest, ToolCall};
use aios_policy::{ApprovalQueue, SessionPolicyEngine};
use aios_runtime::{KernelRuntime, RuntimeConfig, TickInput, TickOutput};
use aios_sandbox::LocalSandboxRunner;
use aios_tools::{ToolDispatcher, ToolRegistry};
use anyhow::Result;

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

    pub async fn tick(
        &self,
        session_id: SessionId,
        objective: impl Into<String>,
        proposed_tool: Option<ToolCall>,
    ) -> Result<TickOutput> {
        self.runtime
            .tick(
                session_id,
                TickInput {
                    objective: objective.into(),
                    proposed_tool,
                },
            )
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
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use aios_model::{Capability, OperatingMode, PolicySet, ToolCall};
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
}
