//! Error types for the Agent OS protocol.

use thiserror::Error;

/// Errors that can occur in kernel operations.
#[derive(Debug, Error)]
pub enum KernelError {
    #[error("capability denied: {0}")]
    CapabilityDenied(String),
    #[error("tool not found: {0}")]
    ToolNotFound(String),
    #[error("approval required: {0}")]
    ApprovalRequired(String),
    #[error("io error: {0}")]
    Io(String),
    #[error("serialization error: {0}")]
    Serialization(String),
    #[error("invalid state: {0}")]
    InvalidState(String),
    #[error("runtime error: {0}")]
    Runtime(String),
    #[error("budget exceeded: {0}")]
    BudgetExceeded(String),
    #[error("sequence conflict: expected {expected}, got {actual}")]
    SequenceConflict { expected: u64, actual: u64 },
}

/// Convenience result type for kernel operations.
pub type KernelResult<T> = Result<T, KernelError>;
