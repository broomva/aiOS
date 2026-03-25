//! # aios-protocol — Canonical Agent OS Protocol
//!
//! This crate defines the shared types, event taxonomy, and trait interfaces
//! that all Agent OS projects (Arcan, Lago, Praxis, Autonomic) depend on.
//!
//! It is intentionally dependency-light (no runtime deps like tokio, axum, or redb)
//! so it can be used as a pure contract crate.
//!
//! ## Module Overview
//!
//! - [`ids`] — Typed ID wrappers (SessionId, EventId, BranchId, BlobHash, etc.)
//! - [`event`] — EventEnvelope + EventKind (~55 variants, forward-compatible)
//! - [`state`] — AgentStateVector, BudgetState (homeostasis vitals)
//! - [`mode`] — OperatingMode, GatingProfile (operating constraints)
//! - [`policy`] — Capability, PolicySet, PolicyEvaluation
//! - [`tool`] — ToolCall, ToolOutcome, ToolDefinition, ToolResult, Tool trait, ToolRegistry
//! - [`sandbox`] — SandboxTier, SandboxLimits, NetworkPolicy
//! - [`memory`] — SoulProfile, Observation, Provenance, MemoryScope
//! - [`session`] — SessionManifest, BranchInfo, CheckpointManifest
//! - [`payment`] — PaymentPort for agent financial operations (x402, MPP)
//! - [`ports`] — Runtime boundary ports (event store, provider, tools, policy, approvals, memory)
//! - [`error`] — KernelError, KernelResult

pub mod error;
pub mod event;
pub mod identity;
pub mod ids;
pub mod memory;
pub mod mode;
pub mod payment;
pub mod policy;
pub mod ports;
pub mod sandbox;
pub mod session;
pub mod state;
pub mod tool;

// Re-export the most commonly used types at the crate root.
pub use error::{KernelError, KernelResult};
pub use event::{
    ActorType, ApprovalDecision, EventActor, EventEnvelope, EventKind, EventRecord, EventSchema,
    LoopPhase, PolicyDecisionKind, RiskLevel, SnapshotType, SpanStatus, SteeringMode, TokenUsage,
};
pub use identity::{AgentIdentityProvider, BasicIdentity};
pub use ids::{
    AgentId, ApprovalId, BlobHash, BranchId, CheckpointId, EventId, HiveTaskId, MemoryId, RunId,
    SeqNo, SessionId, SnapshotId, ToolRunId,
};
pub use memory::{FileProvenance, MemoryScope, Observation, Provenance, SoulProfile};
pub use mode::{GatingProfile, OperatingMode};
pub use payment::{
    PaymentAuthorizationDecision, PaymentAuthorizationRequest, PaymentPort,
    PaymentSettlementReceipt, WalletBalanceInfo,
};
pub use policy::{Capability, PolicyEvaluation, PolicySet};
pub use ports::{
    ApprovalPort, ApprovalRequest, ApprovalResolution, ApprovalTicket, EventRecordStream,
    EventStorePort, ModelCompletion, ModelCompletionRequest, ModelDirective, ModelProviderPort,
    ModelStopReason, PolicyGateDecision, PolicyGatePort, ToolExecutionReport, ToolExecutionRequest,
    ToolHarnessPort,
};
pub use sandbox::{NetworkPolicy, SandboxLimits, SandboxTier};
pub use session::{
    BranchInfo, BranchMergeResult, CheckpointManifest, ModelRouting, SessionManifest,
};
pub use state::{
    AgentStateVector, BlobRef, BudgetState, CanonicalState, MemoryNamespace, PatchApplyError,
    PatchOp, ProvenanceRef, StatePatch, VersionedCanonicalState,
};
pub use tool::{
    Tool, ToolAnnotations, ToolCall, ToolContent, ToolContext, ToolDefinition, ToolError,
    ToolOutcome, ToolRegistry, ToolResult,
};
