//! Kernel-tier types: attribution, context, gate kinds, and the richer
//! kernel-tier error surface.
//!
//! These types are consumed by the (future) `KernelPort` trait (lands in
//! BRO-849) and emitted as payloads on `kernel.*`
//! [`crate::event::EventKind`] variants. This module holds only types — no
//! traits land here in BRO-847.
//!
//! ## Error naming
//!
//! The crate keeps the legacy [`crate::error::KernelError`] as the crate-root
//! re-export for backward compatibility. A richer, kernel-tier
//! [`KernelError`] (this module) carries typed gate and backend variants.
//! The richer error is intentionally NOT re-exported at the crate root in
//! BRO-847 to avoid shadowing the legacy error; reach it via
//! `aios_protocol::kernel::KernelError`. The migration sweep that moves all
//! downstream crates to the richer error is scheduled for BRO-856.

use serde::{Deserialize, Serialize};

use crate::budget::ResourceBudget;
use crate::hypervisor::{BackendError, BackendId, VmId, VmSnapshotId};
use crate::ids::{AgentId, SessionId};

/// Identifies a wallet for on-chain attribution of kernel-emitted events.
///
/// The `address` format is chain-dependent (0x… hex for EVM chains,
/// base58 for Solana, bech32 for Cosmos, etc.). The kernel does not
/// validate the format — backends and downstream gates do.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WalletAttribution {
    pub address: String,
    pub chain: ChainId,
}

/// Chain identifier for the wallet's settlement network.
///
/// Follows CAIP-2 (`<namespace>:<reference>`) format. Helpers are provided
/// for the chains Haima actively supports; other chains can be constructed
/// via [`ChainId::from_caip2`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ChainId(pub String);

impl ChainId {
    /// Base L2 mainnet — Haima's primary settlement chain.
    pub fn base() -> Self {
        Self("eip155:8453".into())
    }

    /// Ethereum mainnet.
    pub fn ethereum() -> Self {
        Self("eip155:1".into())
    }

    /// Construct from a raw CAIP-2 string (e.g. `"eip155:10"` for Optimism).
    pub fn from_caip2(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// View the CAIP-2 string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// ── Context ──────────────────────────────────────────────────────────────────

/// Per-call context flowing through the kernel dispatch path.
///
/// Carries attribution (session, agent, wallet), optional budget hints
/// consulted by the (future) `BudgetGatePort`, and an optional W3C
/// TraceContext for Vigil OTEL propagation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KernelContext {
    pub session_id: SessionId,
    pub agent_id: AgentId,
    pub wallet: WalletAttribution,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_hint: Option<ResourceBudget>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_ctx: Option<TraceContext>,
}

/// W3C TraceContext fields for OTEL propagation.
///
/// See <https://www.w3.org/TR/trace-context/>. The kernel does not parse or
/// validate these fields — it just threads them through to backends and
/// Vigil spans.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceContext {
    pub traceparent: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracestate: Option<String>,
}

// ── Gates & errors ───────────────────────────────────────────────────────────

/// Which gate rejected (or allowed) a kernel operation.
///
/// Used as a discriminator in [`KernelError::GateDenied`] and as a label on
/// `kernel.gate.*` audit events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum GateKind {
    Policy,
    Budget,
    ForkLambda,
    NetworkIsolation,
}

/// Richer kernel-tier error surface.
///
/// Prefer this over the legacy [`crate::error::KernelError`] for new code —
/// it carries typed backend / gate / VM identifiers instead of flattening
/// everything into a string. Bridges [`BackendError`] via `#[from]` so
/// backend failures propagate into kernel-tier results with no boilerplate.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum KernelError {
    #[error("backend not found: {0}")]
    BackendNotFound(BackendId),
    #[error("vm not found: {0}")]
    VmNotFound(VmId),
    #[error("snapshot not found: {0}")]
    SnapshotNotFound(VmSnapshotId),
    #[error("capability unavailable on backend {backend}: {reason}")]
    CapabilityUnavailable { backend: BackendId, reason: String },
    #[error("gate denied ({gate:?}): {reason}")]
    GateDenied { gate: GateKind, reason: String },
    #[error("dispatch timeout after {duration_ms} ms")]
    Timeout { duration_ms: u64 },
    #[error("backend error: {0}")]
    Backend(#[from] BackendError),
    #[error("internal: {0}")]
    Internal(String),
}

/// Convenience alias for kernel-tier results.
pub type KernelResult<T> = Result<T, KernelError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wallet_attribution_roundtrip() {
        let w = WalletAttribution {
            address: "0xabcdef".into(),
            chain: ChainId::base(),
        };
        let json = serde_json::to_string(&w).unwrap();
        let back: WalletAttribution = serde_json::from_str(&json).unwrap();
        assert_eq!(w, back);
    }

    #[test]
    fn chain_id_helpers() {
        assert_eq!(ChainId::base().0, "eip155:8453");
        assert_eq!(ChainId::ethereum().0, "eip155:1");
    }

    #[test]
    fn chain_id_is_transparent() {
        let c = ChainId::base();
        let json = serde_json::to_string(&c).unwrap();
        assert_eq!(json, "\"eip155:8453\"");
        let back: ChainId = serde_json::from_str(&json).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn chain_id_from_caip2() {
        let optimism = ChainId::from_caip2("eip155:10");
        assert_eq!(optimism.as_str(), "eip155:10");
    }

    // ── Context ──

    #[test]
    fn kernel_context_roundtrip_minimal() {
        let ctx = KernelContext {
            session_id: SessionId::from_string("sess-1"),
            agent_id: AgentId::from_string("agent-1"),
            wallet: WalletAttribution {
                address: "0xabc".into(),
                chain: ChainId::base(),
            },
            cost_hint: None,
            trace_ctx: None,
        };
        let json = serde_json::to_string(&ctx).unwrap();
        // Optional fields should be skipped when None.
        assert!(!json.contains("cost_hint"));
        assert!(!json.contains("trace_ctx"));
        let back: KernelContext = serde_json::from_str(&json).unwrap();
        assert_eq!(back.session_id.as_str(), "sess-1");
        assert_eq!(back.wallet.chain, ChainId::base());
    }

    #[test]
    fn kernel_context_roundtrip_with_hints() {
        let ctx = KernelContext {
            session_id: SessionId::from_string("sess-1"),
            agent_id: AgentId::from_string("agent-1"),
            wallet: WalletAttribution {
                address: "0xabc".into(),
                chain: ChainId::base(),
            },
            cost_hint: Some(ResourceBudget {
                max_duration_ms: Some(5_000),
                ..Default::default()
            }),
            trace_ctx: Some(TraceContext {
                traceparent: "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01".into(),
                tracestate: Some("rojo=00f067aa0ba902b7".into()),
            }),
        };
        let json = serde_json::to_string(&ctx).unwrap();
        let back: KernelContext = serde_json::from_str(&json).unwrap();
        assert_eq!(back.cost_hint.unwrap().max_duration_ms, Some(5_000));
        assert_eq!(
            back.trace_ctx.unwrap().traceparent,
            "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01"
        );
    }

    // ── Trace ──

    #[test]
    fn trace_context_tracestate_optional() {
        let tc = TraceContext {
            traceparent: "tp".into(),
            tracestate: None,
        };
        let json = serde_json::to_string(&tc).unwrap();
        assert!(!json.contains("tracestate"));
        let back: TraceContext = serde_json::from_str(&json).unwrap();
        assert_eq!(back.traceparent, "tp");
        assert!(back.tracestate.is_none());
    }

    // ── Gates ──

    #[test]
    fn gate_kind_is_copy_hash_serde() {
        fn assert_copy<T: Copy>() {}
        assert_copy::<GateKind>();
        let json = serde_json::to_string(&GateKind::Policy).unwrap();
        assert_eq!(json, "\"policy\"");
        let back: GateKind = serde_json::from_str("\"network_isolation\"").unwrap();
        assert_eq!(back, GateKind::NetworkIsolation);
    }

    // ── KernelError ──

    #[test]
    fn kernel_error_display_includes_identifiers() {
        let err = KernelError::BackendNotFound(BackendId::from("missing"));
        assert_eq!(err.to_string(), "backend not found: missing");

        let err = KernelError::VmNotFound(VmId::from("vm-42"));
        assert_eq!(err.to_string(), "vm not found: vm-42");

        let err = KernelError::SnapshotNotFound(VmSnapshotId::from("snap-1"));
        assert_eq!(err.to_string(), "snapshot not found: snap-1");
    }

    #[test]
    fn kernel_error_gate_denied_display() {
        let err = KernelError::GateDenied {
            gate: GateKind::Budget,
            reason: "over cap".into(),
        };
        assert!(err.to_string().contains("Budget"));
        assert!(err.to_string().contains("over cap"));
    }

    #[test]
    fn kernel_error_timeout_display() {
        let err = KernelError::Timeout { duration_ms: 1_500 };
        assert_eq!(err.to_string(), "dispatch timeout after 1500 ms");
    }

    #[test]
    fn kernel_result_alias_is_usable() {
        fn good() -> KernelResult<u32> {
            Ok(42)
        }
        assert_eq!(good().unwrap(), 42);
    }

    // ── BackendError bridge ──

    #[test]
    fn kernel_error_from_backend_error() {
        let k: KernelError = BackendError::Internal("oops".into()).into();
        assert!(matches!(k, KernelError::Backend(_)));
        assert!(k.to_string().contains("backend error"));
        assert!(k.to_string().contains("oops"));
    }
}
