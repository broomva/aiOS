//! Bridge between Layer-1 hand-written `aios-protocol` types and Layer-2
//! generated `aios-proto` proto types.
//!
//! Internal substrates use Layer-1 types (newtypes, rich enums, thiserror
//! errors); the wire boundary (lifed, lifegw, soma) speaks Layer-2 proto.
//! This module provides mechanical `From`/`Into` impls so the conversion
//! happens at the boundary, not inside substrate code.
//!
//! # Coverage in M3
//!
//! - 5 canonical identifier types:
//!   [`VmId`](crate::hypervisor::VmId),
//!   [`VmSnapshotId`](crate::hypervisor::VmSnapshotId),
//!   [`BackendId`](crate::hypervisor::BackendId),
//!   [`SessionId`](crate::ids::SessionId),
//!   [`AgentId`](crate::ids::AgentId).
//!
//! M3.5 / M4 extend coverage to: `EventRecord`, `EventKind`, `BackendError`,
//! `KernelError`, `GateKind`, `BudgetDecision`, `PolicyDecision`, `VmSpec`,
//! `VmHandle`, `VmStatus`, `BackendCapabilitySet`, `Mount`, `RuntimeHint`,
//! `BackendSelector`. Until then, compound conversions live in
//! `life-kernel-proto::convert`.
//!
//! # Why this lives in `aios-protocol`
//!
//! Rust's orphan rule requires `impl From<A> for B` to be defined in the
//! crate that owns either `A` or `B`. The Layer-1 types live in this
//! crate; the Layer-2 types live in `aios-proto`. Putting the impls in
//! a third crate (such as `life-kernel-proto`) would violate the rule —
//! hence the bridge owns one side of every conversion and lives next to
//! the Layer-1 types it wraps.

#![allow(clippy::module_name_repetitions)]

use aios_proto::aios::v1 as proto;

use crate::hypervisor;
use crate::ids;

// ── VmId ────────────────────────────────────────────────────────────

impl From<hypervisor::VmId> for proto::VmId {
    fn from(v: hypervisor::VmId) -> Self {
        Self { value: v.0 }
    }
}

impl From<&hypervisor::VmId> for proto::VmId {
    fn from(v: &hypervisor::VmId) -> Self {
        Self { value: v.0.clone() }
    }
}

impl From<proto::VmId> for hypervisor::VmId {
    fn from(p: proto::VmId) -> Self {
        Self(p.value)
    }
}

// ── VmSnapshotId ────────────────────────────────────────────────────

impl From<hypervisor::VmSnapshotId> for proto::VmSnapshotId {
    fn from(v: hypervisor::VmSnapshotId) -> Self {
        Self { value: v.0 }
    }
}

impl From<&hypervisor::VmSnapshotId> for proto::VmSnapshotId {
    fn from(v: &hypervisor::VmSnapshotId) -> Self {
        Self { value: v.0.clone() }
    }
}

impl From<proto::VmSnapshotId> for hypervisor::VmSnapshotId {
    fn from(p: proto::VmSnapshotId) -> Self {
        Self(p.value)
    }
}

// ── BackendId ───────────────────────────────────────────────────────

impl From<hypervisor::BackendId> for proto::BackendId {
    fn from(v: hypervisor::BackendId) -> Self {
        Self { value: v.0 }
    }
}

impl From<&hypervisor::BackendId> for proto::BackendId {
    fn from(v: &hypervisor::BackendId) -> Self {
        Self { value: v.0.clone() }
    }
}

impl From<proto::BackendId> for hypervisor::BackendId {
    fn from(p: proto::BackendId) -> Self {
        Self(p.value)
    }
}

// ── SessionId ───────────────────────────────────────────────────────

impl From<ids::SessionId> for proto::SessionId {
    fn from(v: ids::SessionId) -> Self {
        Self {
            value: v.as_str().to_owned(),
        }
    }
}

impl From<&ids::SessionId> for proto::SessionId {
    fn from(v: &ids::SessionId) -> Self {
        Self {
            value: v.as_str().to_owned(),
        }
    }
}

impl From<proto::SessionId> for ids::SessionId {
    fn from(p: proto::SessionId) -> Self {
        Self::from_string(p.value)
    }
}

// ── AgentId ─────────────────────────────────────────────────────────

impl From<ids::AgentId> for proto::AgentId {
    fn from(v: ids::AgentId) -> Self {
        Self {
            value: v.as_str().to_owned(),
        }
    }
}

impl From<&ids::AgentId> for proto::AgentId {
    fn from(v: &ids::AgentId) -> Self {
        Self {
            value: v.as_str().to_owned(),
        }
    }
}

impl From<proto::AgentId> for ids::AgentId {
    fn from(p: proto::AgentId) -> Self {
        Self::from_string(p.value)
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vm_id_round_trip() {
        let original = hypervisor::VmId::from("vm_12345");
        let wire: proto::VmId = original.clone().into();
        assert_eq!(wire.value, "vm_12345");
        let back: hypervisor::VmId = wire.into();
        assert_eq!(original, back);
    }

    #[test]
    fn vm_snapshot_id_round_trip() {
        let original = hypervisor::VmSnapshotId::from("snap_abcde");
        let wire: proto::VmSnapshotId = original.clone().into();
        assert_eq!(wire.value, "snap_abcde");
        let back: hypervisor::VmSnapshotId = wire.into();
        assert_eq!(original, back);
    }

    #[test]
    fn backend_id_round_trip() {
        let original = hypervisor::BackendId::from("local");
        let wire: proto::BackendId = original.clone().into();
        assert_eq!(wire.value, "local");
        let back: hypervisor::BackendId = wire.into();
        assert_eq!(original, back);
    }

    #[test]
    fn session_id_round_trip() {
        let original = ids::SessionId::from_string("session_abc");
        let wire: proto::SessionId = (&original).into();
        assert_eq!(wire.value, "session_abc");
        let back: ids::SessionId = wire.into();
        assert_eq!(original, back);
    }

    #[test]
    fn agent_id_round_trip() {
        let original = ids::AgentId::from_string("agent_xyz");
        let wire: proto::AgentId = (&original).into();
        assert_eq!(wire.value, "agent_xyz");
        let back: ids::AgentId = wire.into();
        assert_eq!(original, back);
    }
}
