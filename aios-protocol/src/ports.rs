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
    /// Optional system prompt to prepend to the conversation.
    /// Used for skill catalogs, persona blocks, and context compiler output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    /// Tool whitelist from active skill. When set, only these tools are sent to the LLM.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<Vec<String>>,
    /// Conversation history from prior turns in this session.
    /// Built by the runtime from the event journal before each provider call.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conversation_history: Vec<ConversationTurn>,
}

/// A single turn in the conversation history (user message + assistant response).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationTurn {
    pub role: String,
    pub content: String,
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
    /// Optional serialized LLM call envelope/economics record.
    ///
    /// Kept as JSON to avoid making the kernel contract depend on a concrete
    /// observability crate while still allowing runtimes to persist the record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_call_record: Option<serde_json::Value>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

// Re-export the hypervisor trait family for consistency with the other ports.
// Traits are *defined* in [`crate::hypervisor`]; this block only re-exports so
// callers can reach `aios_protocol::ports::HypervisorBackend` alongside the
// other runtime-boundary traits.
pub use crate::budget::{BudgetDecision, BudgetGatePort};
pub use crate::hypervisor::{HypervisorBackend, HypervisorFilesystemExt};
pub use crate::network_isolation::NetworkIsolationPort;

// ── KernelPort ───────────────────────────────────────────────────────────────

// Scoped sub-module so the richer kernel-tier `KernelResult` (and fresh
// imports of `ToolResult` + hypervisor types) coexist with the legacy
// `error::KernelResult` used by the ports above without a rename shim.
// Exposed through the `pub use kernel_port::KernelPort;` below.
mod kernel_port {
    use super::ToolCall;
    use crate::hypervisor::{ForkSpec, VmHandle, VmSnapshotHandle, VmSpec};
    use crate::kernel::{KernelContext, KernelResult};
    use crate::tool::ToolResult;

    /// High-level Tool-ABI dispatch into an isolated VM.
    ///
    /// This is the trait callers depend on — `arcand` today, Life Runtime
    /// library in Spec B tomorrow — and the canonical surface `lifed`
    /// implements. Everything lower-level (raw VM lifecycle, shell exec,
    /// filesystem) sits behind [`crate::hypervisor::HypervisorBackend`] and
    /// is orchestrated by the `lifed` implementation of this trait.
    ///
    /// ## Shared-ref only
    ///
    /// None of these methods take `&mut self`: callers hold
    /// `Arc<dyn KernelPort>` and dispatch concurrently across many VMs.
    /// State lives inside the implementation (typically behind interior
    /// mutability or a dedicated runtime actor); the trait itself is a
    /// read-only handle.
    ///
    /// ## Ownership semantics
    ///
    /// - [`destroy`](KernelPort::destroy) takes `VmHandle` by value
    ///   because the handle must not be reused after the call — callers
    ///   that still need the ID should clone it before dispatching.
    /// - [`create_vm`](KernelPort::create_vm) / [`fork`](KernelPort::fork)
    ///   take `KernelContext` by value because the context is per-call
    ///   and conceptually moves into the resulting lifetime of the new VM.
    /// - Hot-path methods ([`dispatch`](KernelPort::dispatch),
    ///   [`snapshot`](KernelPort::snapshot),
    ///   [`hibernate`](KernelPort::hibernate),
    ///   [`resume`](KernelPort::resume)) take handles by shared reference
    ///   so the same VM can be kept alive across many dispatches.
    ///
    /// ## Error surface
    ///
    /// Returns [`crate::kernel::KernelResult`] (the richer kernel-tier
    /// error), *not* the legacy [`crate::error::KernelResult`] used by
    /// the older ports (`EventStorePort`, `ModelProviderPort`, …). Those
    /// ports migrate in BRO-856.
    #[async_trait::async_trait]
    pub trait KernelPort: Send + Sync {
        /// Provision a new VM from `spec` under the attribution / budget
        /// hints carried by `ctx`. Returns a live handle; the VM may
        /// still be in [`crate::hypervisor::VmStatus::Starting`] when the
        /// call returns.
        async fn create_vm(&self, spec: VmSpec, ctx: KernelContext) -> KernelResult<VmHandle>;

        /// Dispatch a [`ToolCall`] against a running VM and return its
        /// [`ToolResult`]. Hot-path entry point gated by
        /// [`crate::budget::BudgetGatePort`] and — when the backend has
        /// network I/O — [`crate::network_isolation::NetworkIsolationPort`].
        async fn dispatch(
            &self,
            vm: &VmHandle,
            call: ToolCall,
            ctx: &KernelContext,
        ) -> KernelResult<ToolResult>;

        /// Snapshot the current VM state under the human-readable `name`
        /// (used for fork labels, audit events, and operator UX). The
        /// returned handle can be passed to [`fork`](KernelPort::fork) or
        /// archived for later restore.
        async fn snapshot(&self, vm: &VmHandle, name: &str) -> KernelResult<VmSnapshotHandle>;

        /// Fork a new VM from `snapshot` with the overrides in `spec`.
        /// Fork gating is stricter than dispatch gating because unbounded
        /// forks can amplify cost exponentially — see
        /// [`crate::budget::BudgetGatePort::check_fork`].
        async fn fork(
            &self,
            snapshot: &VmSnapshotHandle,
            spec: ForkSpec,
            ctx: KernelContext,
        ) -> KernelResult<VmHandle>;

        /// Pause the VM and persist its state. Backends that do not
        /// support hibernation surface
        /// [`crate::kernel::KernelError::Backend`] wrapping
        /// [`crate::hypervisor::BackendError::NotSupported`].
        async fn hibernate(&self, vm: &VmHandle) -> KernelResult<()>;

        /// Resume a previously hibernated VM. Returns the live handle
        /// for the resumed instance (which may differ from the
        /// pre-hibernate handle, e.g. after a controller restart).
        async fn resume(&self, vm: &VmHandle) -> KernelResult<VmHandle>;

        /// Destroy the VM. Takes the handle by value so stale handles
        /// cannot be re-used after destruction. MUST succeed even if the
        /// VM is already stopped.
        async fn destroy(&self, vm: VmHandle) -> KernelResult<()>;
    }
}

pub use kernel_port::KernelPort;

use crate::session::{CreateSessionRequest, SessionFilter, SessionManifest, TickInput, TickOutput};

/// High-level session lifecycle port.
///
/// Implementors provide create/get/list/tick/stream/close over the session tier.
/// `arcand` is the reference implementation; `life-kernel-facade` consumes this
/// trait through `Arc<dyn SessionPort>`.
#[async_trait]
pub trait SessionPort: Send + Sync {
    async fn create(&self, req: CreateSessionRequest) -> KernelResult<SessionManifest>;
    async fn get(&self, id: SessionId) -> KernelResult<SessionManifest>;
    async fn list(&self, filter: SessionFilter) -> KernelResult<Vec<SessionManifest>>;
    async fn tick(&self, id: SessionId, input: TickInput) -> KernelResult<TickOutput>;
    async fn stream_events(
        &self,
        id: SessionId,
        branch: BranchId,
        after_sequence: u64,
    ) -> KernelResult<EventRecordStream>;
    async fn close(&self, id: SessionId, reason: String) -> KernelResult<()>;
}

#[cfg(test)]
mod session_port_tests {
    use super::*;

    #[test]
    fn _assert_session_port_dyn_safe() {
        fn _dyn_safe(_p: &dyn SessionPort) {}
    }
}

#[cfg(test)]
mod trait_tests {
    use super::*;

    // Compile-time assertion that `KernelPort` is dyn-compatible — the whole
    // reason we use `#[async_trait]` instead of native async fn. Callers
    // hold `Arc<dyn KernelPort>`; if this ever regresses, every caller
    // breaks.
    #[allow(dead_code)]
    fn _assert_dyn(_: &dyn KernelPort) {}

    #[test]
    fn kernel_port_is_dyn_compatible() {
        // If this compiles, the trait is dyn-compatible.
        #[allow(dead_code)]
        fn _use_it(_: &dyn KernelPort) {}
    }
}
