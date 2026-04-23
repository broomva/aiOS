//! Network isolation types + per-VM enforcement port for egress metering.
//!
//! Distinct from [`crate::sandbox::NetworkPolicy`] (which is policy
//! *declaration*). This module holds the record types reported by a VM's
//! network hook and the [`NetworkIsolationPort`] trait that *enforces*
//! policy per-VM at runtime and records egress for metering.
//!
//! BRO-847 seeded [`EgressTarget`] and [`EgressProtocol`]; BRO-849 adds the
//! trait that consumes them.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::hypervisor::VmHandle;
use crate::kernel::KernelResult;
use crate::sandbox::NetworkPolicy;

/// An egress destination observed by the VM's network hook. Used for
/// metering and for emitting `network.egress` audit events.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EgressTarget {
    /// Destination host (hostname or IP literal).
    pub host: String,
    /// Destination port.
    pub port: u16,
    /// L4 protocol used.
    pub protocol: EgressProtocol,
}

/// Layer-4 protocol family for an observed egress flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum EgressProtocol {
    Tcp,
    Udp,
    Icmp,
}

// ── Port ─────────────────────────────────────────────────────────────────────

/// Per-VM network isolation + egress accounting.
///
/// Called by [`crate::ports::KernelPort`] implementations on two paths:
///
/// - Once at VM start, via [`apply`](NetworkIsolationPort::apply), to
///   materialise the [`NetworkPolicy`] declared on the VM spec (e.g. wire an
///   eBPF allowlist, set up a userspace proxy, or no-op for disabled
///   networking).
/// - On every observed egress flow, via
///   [`record_egress`](NetworkIsolationPort::record_egress), so metering,
///   audit events, and budget gates can cross-reference actual network
///   usage against declared policy.
///
/// MVS default impl is `NoOpNetworkIsolation` (Phase 1, logs but allows
/// all). `AllowListNetworkIsolation` lands in Phase 4.
/// `EbpfNetworkIsolation` (CubeVS pattern) is Phase 6.
#[async_trait]
pub trait NetworkIsolationPort: Send + Sync {
    /// Apply a network policy to a VM. Typically called exactly once at VM
    /// start, after [`crate::ports::KernelPort::create_vm`] returns.
    ///
    /// Returning `Err` aborts VM bring-up — the caller is expected to
    /// propagate the error back to the originating dispatch.
    async fn apply(&self, vm: &VmHandle, policy: &NetworkPolicy) -> KernelResult<()>;

    /// Record an observed egress event. Called by the backend's network
    /// monitoring hook for every flow that reaches a destination. `bytes`
    /// is the payload size (egress direction only); `dst` identifies the
    /// destination for audit + allowlist checks.
    async fn record_egress(
        &self,
        vm: &VmHandle,
        bytes: u64,
        dst: &EgressTarget,
    ) -> KernelResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn egress_target_roundtrip() {
        let t = EgressTarget {
            host: "api.example.com".into(),
            port: 443,
            protocol: EgressProtocol::Tcp,
        };
        let json = serde_json::to_string(&t).unwrap();
        let back: EgressTarget = serde_json::from_str(&json).unwrap();
        assert_eq!(t, back);
    }

    #[test]
    fn egress_protocol_serde_snake_case() {
        assert_eq!(
            serde_json::to_string(&EgressProtocol::Tcp).unwrap(),
            "\"tcp\""
        );
        let back: EgressProtocol = serde_json::from_str("\"udp\"").unwrap();
        assert_eq!(back, EgressProtocol::Udp);
        let back: EgressProtocol = serde_json::from_str("\"icmp\"").unwrap();
        assert_eq!(back, EgressProtocol::Icmp);
    }

    #[test]
    fn egress_target_hashable() {
        use std::collections::HashSet;
        let t = EgressTarget {
            host: "x".into(),
            port: 1,
            protocol: EgressProtocol::Tcp,
        };
        let mut s = HashSet::new();
        s.insert(t.clone());
        assert!(s.contains(&t));
    }

    // Compile-time assertion that `NetworkIsolationPort` is dyn-compatible —
    // the whole reason we use `#[async_trait]` instead of native async fn.
    // Downstream callers hold `Arc<dyn NetworkIsolationPort>` in the kernel
    // registry; if this ever regresses, the entire registry type breaks.
    #[allow(dead_code)]
    fn _assert_dyn(_: &dyn NetworkIsolationPort) {}

    #[test]
    fn network_isolation_port_is_dyn_compatible() {
        // If this compiles, the trait is dyn-compatible.
        #[allow(dead_code)]
        fn _use_it(_: &dyn NetworkIsolationPort) {}
    }
}
