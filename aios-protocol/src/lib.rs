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
//! - [`finance`] — Finance DTOs (wallet, authorization, settlement, transaction history, usage)
//! - [`payment`] — PaymentPort for agent financial operations (x402, MPP)
//! - [`ports`] — Runtime boundary ports (event store, provider, tools, policy, approvals,
//!   memory, KernelPort for high-level Tool-ABI dispatch)
//! - [`rcs`] — Recursive Controlled Systems traits (Level, RecursiveControlledSystem, StabilityBudget)
//! - [`error`] — KernelError, KernelResult (legacy kernel-tier error; see [`kernel`] for the
//!   richer replacement landing alongside the hypervisor substrate)
//! - [`budget`] — ResourceBudget, ResourceUsage, UsageConfidence, BudgetDecision,
//!   BudgetGatePort (kernel-tier metering + cost gate trait)
//! - [`kernel`] — WalletAttribution, ChainId, KernelContext, TraceContext, GateKind, and the
//!   richer kernel-tier KernelError (reachable as `aios_protocol::kernel::KernelError`)
//! - [`hypervisor`] — VM substrate types (VmHandle, VmSpec, VmSnapshotHandle, ForkSpec,
//!   ExecRequest, …) plus the HypervisorBackend + HypervisorFilesystemExt traits and
//!   the BackendError / BackendCapabilitySet surface implemented by `arcan-provider-*`
//! - [`network_isolation`] — EgressTarget, EgressProtocol, NetworkIsolationPort
//!   (egress metering + per-VM enforcement)
//! - [`proto_bridge`] — `From`/`Into` impls between Layer-1 hand-written
//!   types and the Layer-2 generated types in `aios-proto::aios::v1::*`
//!   (M3 / BRO-928). Currently covers the 5 canonical identifier types.

pub mod billing;
pub mod blob;
pub mod budget;
pub mod error;
pub mod evaluation;
pub mod event;
pub mod finance;
pub mod homeostasis;
pub mod hypervisor;
pub mod identity;
pub mod ids;
pub mod kernel;
pub mod knowledge;
pub mod memory;
pub mod mode;
pub mod network_isolation;
pub mod payment;
pub mod policy;
pub mod ports;
pub mod proto_bridge;
pub mod rcs;
pub mod relay;
pub mod sandbox;
pub mod session;
pub mod state;
pub mod tool;
pub mod world;

// Re-export the most commonly used types at the crate root.
pub use budget::{BudgetDecision, BudgetGatePort, ResourceBudget, ResourceUsage, UsageConfidence};
pub use error::{KernelError, KernelResult};
pub use event::{
    ActorType, ApprovalDecision, EventActor, EventEnvelope, EventKind, EventRecord, EventSchema,
    KernelDispatchCompleted, KernelDispatchDenied, KernelDispatchStarted, KernelEgressRecorded,
    KernelForkDenied, KernelPolicyViolated, KernelUsageRecorded, KernelVmCreated,
    KernelVmDestroyed, KernelVmForked, KernelVmHibernated, KernelVmResumed, KernelVmSnapshotted,
    LoopPhase, PolicyDecisionKind, RiskLevel, SnapshotType, SpanStatus, SteeringMode, TokenUsage,
};
pub use hypervisor::{
    BackendCapabilitySet, BackendError, BackendId, BackendSelector, ExecRequest, ExecResult,
    FileWrite, ForkSpec, HypervisorBackend, HypervisorFilesystemExt, Mount, RuntimeHint, VmHandle,
    VmId, VmInfo, VmResources, VmSnapshotHandle, VmSnapshotId, VmSpec, VmSpecOverrides, VmStatus,
};
pub use identity::{AgentIdentityProvider, BasicIdentity};
pub use ids::{
    AgentId, ApprovalId, BlobHash, BranchId, CheckpointId, EventId, HiveTaskId, MemoryId, RunId,
    SeqNo, SessionId, SnapshotId, ToolRunId,
};
// Note: richer kernel-tier `kernel::KernelError` / `kernel::KernelResult` are NOT
// re-exported at the crate root to avoid shadowing the legacy `error::KernelError`
// above; downstream crates should use `aios_protocol::kernel::KernelError` until the
// migration sweep in BRO-856.
pub use kernel::{ChainId, GateKind, KernelContext, TraceContext, WalletAttribution};
pub use memory::{FileProvenance, MemoryScope, Observation, Provenance, SoulProfile};
pub use mode::{GatingProfile, OperatingMode};
pub use network_isolation::{EgressProtocol, EgressTarget, NetworkIsolationPort};
pub use payment::{
    PaymentAuthorizationDecision, PaymentAuthorizationRequest, PaymentPort,
    PaymentSettlementReceipt, WalletBalanceInfo,
};
pub use policy::{Capability, PolicyEvaluation, PolicySet, SubscriptionTier};
pub use ports::{
    ApprovalPort, ApprovalRequest, ApprovalResolution, ApprovalTicket, ConversationTurn,
    EventRecordStream, EventStorePort, KernelPort, ModelCompletion, ModelCompletionRequest,
    ModelDirective, ModelProviderPort, ModelStopReason, PolicyGateDecision, PolicyGatePort,
    ToolExecutionReport, ToolExecutionRequest, ToolHarnessPort,
};
pub use rcs::{
    L0, L1, L2, L3, Level, LyapunovCandidate, RecursiveControlledSystem, StabilityBreakdown,
    StabilityBudget,
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
