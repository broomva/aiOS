//! # aios-protocol — Canonical Agent OS Protocol
//!
//! This crate defines the shared types, event taxonomy, and trait interfaces
//! that all Agent OS projects (Arcan, Lago, Autonomic) depend on.
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
//! - [`tool`] — ToolCall, ToolOutcome
//! - [`memory`] — SoulProfile, Observation, Provenance, MemoryScope
//! - [`session`] — SessionManifest, BranchInfo, CheckpointManifest
//! - [`error`] — KernelError, KernelResult

pub mod error;
pub mod event;
pub mod ids;
pub mod memory;
pub mod mode;
pub mod policy;
pub mod session;
pub mod state;
pub mod tool;

// Re-export the most commonly used types at the crate root.
pub use error::{KernelError, KernelResult};
pub use event::{
    ApprovalDecision, EventEnvelope, EventKind, EventRecord, LoopPhase, PolicyDecisionKind,
    RiskLevel, SnapshotType, SpanStatus, TokenUsage,
};
pub use ids::{
    ApprovalId, BlobHash, BranchId, CheckpointId, EventId, MemoryId, RunId, SeqNo, SessionId,
    SnapshotId, ToolRunId,
};
pub use memory::{FileProvenance, MemoryScope, Observation, Provenance, SoulProfile};
pub use mode::{GatingProfile, OperatingMode};
pub use policy::{Capability, PolicyEvaluation, PolicySet};
pub use session::{
    BranchInfo, BranchMergeResult, CheckpointManifest, ModelRouting, SessionManifest,
};
pub use state::{AgentStateVector, BudgetState};
pub use tool::{ToolCall, ToolOutcome};
