//! Resource budgets and usage accounting for kernel-tier metering.
//!
//! This module holds the type surface plus the [`BudgetGatePort`] trait
//! consulted before every [`crate::ports::KernelPort`] dispatch or fork, and
//! emitted as payload on `kernel.dispatch.completed` events. Types are
//! additive-only: consumers that do not care about budgets treat every field
//! as optional and ignore any variant they do not recognize.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::hypervisor::{ForkSpec, VmHandle};
use crate::kernel::KernelContext;
use crate::ports::ApprovalTicket;

/// Resource limits that can constrain a single dispatch or fork.
///
/// All fields are optional — `None` means no limit for that dimension. The
/// type is used both as a "cost hint" supplied on a `KernelContext` and as
/// the authoritative cap checked by a `BudgetGatePort`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceBudget {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_cpu_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_mem_kb: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_egress_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_syscalls: Option<u64>,
}

/// Actual resource consumption reported by a backend after a dispatch.
///
/// Field accuracy varies by backend — see [`UsageConfidence`] for the
/// accompanying signal. Consumers should treat fields with
/// `UsageConfidence::Unknown` as missing rather than zero.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceUsage {
    pub cpu_ms: u64,
    pub mem_peak_kb: u64,
    pub egress_bytes: u64,
    pub duration_ms: u64,
    pub syscall_count: u64,
    pub confidence: UsageConfidence,
}

/// Per-backend accuracy signal for [`ResourceUsage`] fields.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum UsageConfidence {
    /// Actually measured at the hypervisor/syscall boundary.
    Measured,
    /// Approximated from available proxies (e.g., wall-clock for CPU).
    #[default]
    Estimated,
    /// Backend did not report this field.
    Unknown,
}

// ── Gate ─────────────────────────────────────────────────────────────────────

/// Decision returned by a [`BudgetGatePort`] check.
///
/// Serialized with `#[serde(tag = "decision", rename_all = "snake_case")]` so
/// the wire form is a tagged object — e.g. `{"decision":"allow"}`,
/// `{"decision":"deny","reason":"…","gate_id":"…"}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
#[non_exhaustive]
pub enum BudgetDecision {
    /// Proceed with the dispatch / fork. No budget action needed.
    Allow,
    /// Reject the dispatch / fork outright. `gate_id` identifies which
    /// concrete gate implementation raised the denial (e.g.
    /// `"session-budget"`, `"rcs-lambda"`), and `reason` is a human-readable
    /// explanation suitable for inclusion in `kernel.gate.*` audit events.
    Deny {
        /// Human-readable reason for the denial.
        reason: String,
        /// Stable identifier of the gate that issued the denial.
        gate_id: String,
    },
    /// Defer to a human / governance approver. The enclosed
    /// [`ApprovalTicket`] is routed through the standard
    /// [`crate::ports::ApprovalPort`] queue; the caller MUST NOT dispatch
    /// until the ticket resolves positively.
    RequireApproval {
        /// Ticket already enqueued on the approval queue.
        ticket: ApprovalTicket,
    },
}

/// Cost- and budget-aware gate consulted before every
/// [`crate::ports::KernelPort`] dispatch and fork.
///
/// The gate is advisory from the caller's perspective but authoritative from
/// the kernel's: `soma` will translate a [`BudgetDecision::Deny`] into
/// [`crate::kernel::KernelError::GateDenied`] with
/// [`crate::kernel::GateKind::Budget`], and
/// [`BudgetDecision::RequireApproval`] into an approval-pending stall.
///
/// Implementations are expected to be cheap and pure (no I/O on the hot
/// path). MVS default impl is `NoOpBudgetGate` (Phase 1, permits everything).
/// `SessionBudgetGate` lands in Phase 4. `RcsLambdaBudgetGate` is Phase 6.
#[async_trait]
pub trait BudgetGatePort: Send + Sync {
    /// Check whether a dispatch should proceed under `ctx` with the given
    /// `cost_hint`. Called on the hot path of every
    /// [`crate::ports::KernelPort::dispatch`] call.
    async fn check_dispatch(
        &self,
        ctx: &KernelContext,
        cost_hint: &ResourceBudget,
    ) -> BudgetDecision;

    /// Check whether forking `parent` with `spec` should proceed under `ctx`.
    /// Called on every [`crate::ports::KernelPort::fork`] call; fork gating
    /// is typically stricter than dispatch gating because forks can amplify
    /// cost exponentially.
    async fn check_fork(
        &self,
        parent: &VmHandle,
        spec: &ForkSpec,
        ctx: &KernelContext,
    ) -> BudgetDecision;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_budget_defaults_to_no_limits() {
        let b = ResourceBudget::default();
        assert!(b.max_cpu_ms.is_none());
        assert!(b.max_mem_kb.is_none());
        assert!(b.max_egress_bytes.is_none());
        assert!(b.max_duration_ms.is_none());
        assert!(b.max_syscalls.is_none());
    }

    #[test]
    fn resource_budget_default_omits_none_fields() {
        // Confirms serde(skip_serializing_if) is wired correctly so a fully
        // unconstrained budget does not pollute the wire format.
        let b = ResourceBudget::default();
        let json = serde_json::to_string(&b).unwrap();
        assert_eq!(json, "{}");
    }

    #[test]
    fn resource_budget_partial_roundtrip() {
        let b = ResourceBudget {
            max_cpu_ms: Some(1_000),
            max_duration_ms: Some(30_000),
            ..Default::default()
        };
        let json = serde_json::to_string(&b).unwrap();
        let back: ResourceBudget = serde_json::from_str(&json).unwrap();
        assert_eq!(b, back);
    }

    #[test]
    fn resource_usage_roundtrip() {
        let u = ResourceUsage {
            cpu_ms: 100,
            mem_peak_kb: 2048,
            egress_bytes: 0,
            duration_ms: 120,
            syscall_count: 42,
            confidence: UsageConfidence::Measured,
        };
        let json = serde_json::to_string(&u).unwrap();
        let back: ResourceUsage = serde_json::from_str(&json).unwrap();
        assert_eq!(u, back);
    }

    #[test]
    fn usage_confidence_defaults_to_estimated() {
        assert_eq!(UsageConfidence::default(), UsageConfidence::Estimated);
    }

    #[test]
    fn usage_confidence_serde_snake_case() {
        let json = serde_json::to_string(&UsageConfidence::Measured).unwrap();
        assert_eq!(json, "\"measured\"");
        let back: UsageConfidence = serde_json::from_str("\"unknown\"").unwrap();
        assert_eq!(back, UsageConfidence::Unknown);
    }
}

#[cfg(test)]
mod gate_tests {
    use super::*;

    #[test]
    fn budget_decision_allow_roundtrip() {
        let d = BudgetDecision::Allow;
        let json = serde_json::to_string(&d).unwrap();
        let back: BudgetDecision = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }

    #[test]
    fn budget_decision_deny_roundtrip() {
        let d = BudgetDecision::Deny {
            reason: "over budget".into(),
            gate_id: "session-budget".into(),
        };
        let json = serde_json::to_string(&d).unwrap();
        let back: BudgetDecision = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }

    #[test]
    fn budget_decision_serde_tag_is_snake_case() {
        // The tag field is literally named "decision" and variant names are
        // snake-cased. Assert both on the serialized wire form to lock the
        // contract down; downstream event consumers depend on it.
        let allow = serde_json::to_string(&BudgetDecision::Allow).unwrap();
        assert_eq!(allow, r#"{"decision":"allow"}"#);

        let deny = serde_json::to_string(&BudgetDecision::Deny {
            reason: "r".into(),
            gate_id: "g".into(),
        })
        .unwrap();
        assert!(deny.starts_with(r#"{"decision":"deny","#));
    }

    #[test]
    fn budget_decision_require_approval_roundtrip() {
        // Verifies that BudgetDecision::RequireApproval serializes cleanly —
        // requires ApprovalTicket to already derive Serialize + Deserialize,
        // which it does (see ports.rs).
        use crate::ids::{ApprovalId, SessionId};
        use crate::policy::Capability;
        use chrono::{DateTime, Utc};

        // Fixed timestamp so the assertion is deterministic.
        let created_at: DateTime<Utc> = "2026-04-23T00:00:00Z".parse().unwrap();
        let ticket = ApprovalTicket {
            approval_id: ApprovalId::from_string("app-1"),
            session_id: SessionId::from_string("sess-1"),
            call_id: "call-1".into(),
            tool_name: "shell".into(),
            capability: Capability::new("exec:cmd:echo"),
            reason: "high-risk".into(),
            created_at,
        };
        let d = BudgetDecision::RequireApproval { ticket };
        let json = serde_json::to_string(&d).unwrap();
        let back: BudgetDecision = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }

    // Compile-time assertion that `BudgetGatePort` is dyn-compatible — the
    // whole reason we use `#[async_trait]` instead of native async fn.
    #[allow(dead_code)]
    fn _assert_dyn(_: &dyn BudgetGatePort) {}
}
