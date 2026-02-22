//! Canonical runtime ports for Agent OS integrations.
//!
//! These traits define the only allowed runtime boundary between the kernel
//! engine and external implementations (event stores, model providers, tool
//! harnesses, policy engines, approval systems, and memory backends).
//!
//! Object-safety note:
//! - Traits use `async-trait` for async dyn-dispatch.
//! - Streaming uses boxed trait objects (`EventRecordStream`).

use crate::error::KernelResult;
use crate::event::{EventRecord, TokenUsage};
use crate::ids::{ApprovalId, BranchId, RunId, SessionId, ToolRunId};
use crate::memory::{Observation, SoulProfile};
use crate::policy::Capability;
use crate::tool::{ToolCall, ToolOutcome};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures_util::stream::BoxStream;
use serde::{Deserialize, Serialize};

pub type EventRecordStream = BoxStream<'static, KernelResult<EventRecord>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCompletionRequest {
    pub session_id: SessionId,
    pub branch_id: BranchId,
    pub run_id: RunId,
    pub step_index: u32,
    pub objective: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposed_tool: Option<ToolCall>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ModelDirective {
    TextDelta {
        delta: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        index: Option<u32>,
    },
    Message {
        role: String,
        content: String,
    },
    ToolCall {
        call: ToolCall,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelStopReason {
    Completed,
    ToolCall,
    MaxIterations,
    Cancelled,
    Error,
    Other(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCompletion {
    pub provider: String,
    pub model: String,
    #[serde(default)]
    pub directives: Vec<ModelDirective>,
    pub stop_reason: ModelStopReason,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<TokenUsage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_answer: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecutionRequest {
    pub session_id: SessionId,
    pub workspace_root: String,
    pub call: ToolCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecutionReport {
    pub tool_run_id: ToolRunId,
    pub call_id: String,
    pub tool_name: String,
    pub exit_status: i32,
    pub duration_ms: u64,
    pub outcome: ToolOutcome,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyGateDecision {
    #[serde(default)]
    pub allowed: Vec<Capability>,
    #[serde(default)]
    pub requires_approval: Vec<Capability>,
    #[serde(default)]
    pub denied: Vec<Capability>,
}

impl PolicyGateDecision {
    pub fn is_allowed_now(&self) -> bool {
        self.denied.is_empty() && self.requires_approval.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub session_id: SessionId,
    pub call_id: String,
    pub tool_name: String,
    pub capability: Capability,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalTicket {
    pub approval_id: ApprovalId,
    pub session_id: SessionId,
    pub call_id: String,
    pub tool_name: String,
    pub capability: Capability,
    pub reason: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalResolution {
    pub approval_id: ApprovalId,
    pub approved: bool,
    pub actor: String,
    pub resolved_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryQuery {
    pub limit: usize,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

impl Default for MemoryQuery {
    fn default() -> Self {
        Self {
            limit: 256,
            tags: Vec::new(),
            text: None,
        }
    }
}

#[async_trait]
pub trait EventStorePort: Send + Sync {
    async fn append(&self, event: EventRecord) -> KernelResult<EventRecord>;
    async fn read(
        &self,
        session_id: SessionId,
        branch_id: BranchId,
        from_sequence: u64,
        limit: usize,
    ) -> KernelResult<Vec<EventRecord>>;
    async fn head(&self, session_id: SessionId, branch_id: BranchId) -> KernelResult<u64>;
    async fn subscribe(
        &self,
        session_id: SessionId,
        branch_id: BranchId,
        after_sequence: u64,
    ) -> KernelResult<EventRecordStream>;
}

#[async_trait]
pub trait ModelProviderPort: Send + Sync {
    async fn complete(&self, request: ModelCompletionRequest) -> KernelResult<ModelCompletion>;
}

#[async_trait]
pub trait ToolHarnessPort: Send + Sync {
    async fn execute(&self, request: ToolExecutionRequest) -> KernelResult<ToolExecutionReport>;
}

#[async_trait]
pub trait PolicyGatePort: Send + Sync {
    async fn evaluate(
        &self,
        session_id: SessionId,
        requested: Vec<Capability>,
    ) -> KernelResult<PolicyGateDecision>;

    async fn set_policy(
        &self,
        _session_id: SessionId,
        _policy: crate::policy::PolicySet,
    ) -> KernelResult<()> {
        Ok(())
    }
}

#[async_trait]
pub trait ApprovalPort: Send + Sync {
    async fn enqueue(&self, request: ApprovalRequest) -> KernelResult<ApprovalTicket>;
    async fn list_pending(&self, session_id: SessionId) -> KernelResult<Vec<ApprovalTicket>>;
    async fn resolve(
        &self,
        approval_id: ApprovalId,
        approved: bool,
        actor: String,
    ) -> KernelResult<ApprovalResolution>;
}

#[async_trait]
pub trait MemoryPort: Send + Sync {
    async fn load_soul(&self, session_id: SessionId) -> KernelResult<SoulProfile>;
    async fn save_soul(&self, session_id: SessionId, soul: SoulProfile) -> KernelResult<()>;
    async fn append_observation(
        &self,
        session_id: SessionId,
        observation: Observation,
    ) -> KernelResult<()>;
    async fn query_observations(
        &self,
        session_id: SessionId,
        query: MemoryQuery,
    ) -> KernelResult<Vec<Observation>>;
}
