//! Homeostasis DTOs — wire-facing types served by autonomicd.
//!
//! These types are the **HTTP API surface** for the homeostasis subsystem.
//! They are intentionally leaner than `autonomic-core`'s internal projection
//! state (`HomeostaticState` there is a rich fold-accumulator with per-pillar
//! detail). The types here are what callers of `/gating` and `/projection`
//! endpoints receive over the wire.
//!
//! `autonomic-api-schema` re-exports this module so client crates
//! (`life-kernel-facade`, etc.) depend only on `aios-protocol`.

use crate::ids::SessionId;
use serde::{Deserialize, Serialize};

/// Wire-facing snapshot of an agent's three-pillar homeostatic health.
///
/// Returned by `GET /projection/{session_id}` on autonomicd.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct HomeostaticStateDto {
    pub session: SessionId,
    pub operational: PillarStateDto,
    pub cognitive: PillarStateDto,
    pub economic: PillarStateDto,
    pub sampled_at: chrono::DateTime<chrono::Utc>,
}

/// Per-pillar health summary included in [`HomeostaticStateDto`].
#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct PillarStateDto {
    pub mode: EconomicMode,
    pub budget: BudgetStateDto,
    /// Opaque extra signals — forward-compatible extension point.
    #[serde(default)]
    pub signals: serde_json::Value,
}

/// Lightweight budget snapshot for the wire API.
///
/// All fields are `Option` so new fields can be added without breaking
/// existing deserializers. Use `#[serde(default)]` on the parent for
/// forward-compat deserialization.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub struct BudgetStateDto {
    pub remaining_cpu_ms: Option<u64>,
    pub remaining_mem_kb: Option<u64>,
    pub remaining_egress_bytes: Option<u64>,
    pub remaining_tokens: Option<u64>,
    pub remaining_tool_calls: Option<u64>,
    #[serde(default)]
    pub floor_violated: bool,
}

/// Economic operating mode — mirrors `autonomic_core::economic::EconomicMode`
/// but defined here so wire clients don't depend on the autonomic crate.
///
/// The variants must stay in sync with autonomic-core's `EconomicMode`.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum EconomicMode {
    /// Balance > 2× monthly burn. Full autonomy.
    #[default]
    Sovereign,
    /// 1–2× monthly burn. Prefer cheaper models, limit expensive tools.
    Conserving,
    /// 0–1× monthly burn. Cheapest model only, no expensive tools.
    Hustle,
    /// Balance ≤ 0. Skip LLM calls, heartbeats only.
    Hibernate,
}

/// A projected future homeostatic state, returned by the streaming endpoint.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct HomeostaticProjectionDto {
    pub session: SessionId,
    pub at: chrono::DateTime<chrono::Utc>,
    /// How far ahead this projection looks, in seconds.
    pub horizon_seconds: i64,
    pub projected_state: HomeostaticStateDto,
    pub hysteresis_gate_open: bool,
}

impl HomeostaticProjectionDto {
    /// Convenience accessor returning the horizon as a `chrono::Duration`.
    pub fn horizon(&self) -> chrono::Duration {
        chrono::Duration::seconds(self.horizon_seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn economic_mode_serde_roundtrip() {
        for mode in [
            EconomicMode::Sovereign,
            EconomicMode::Conserving,
            EconomicMode::Hustle,
            EconomicMode::Hibernate,
        ] {
            let json = serde_json::to_string(&mode).unwrap();
            let back: EconomicMode = serde_json::from_str(&json).unwrap();
            assert_eq!(mode, back);
        }
    }

    #[test]
    fn budget_state_dto_defaults() {
        let b = BudgetStateDto::default();
        assert!(!b.floor_violated);
        assert!(b.remaining_tokens.is_none());
    }

    #[test]
    fn projection_horizon_helper() {
        use crate::ids::SessionId;
        let dummy_session = SessionId::new_uuid();
        let pillar = PillarStateDto {
            mode: EconomicMode::Sovereign,
            budget: BudgetStateDto::default(),
            signals: serde_json::Value::Null,
        };
        let state = HomeostaticStateDto {
            session: dummy_session.clone(),
            operational: pillar.clone(),
            cognitive: pillar.clone(),
            economic: pillar,
            sampled_at: chrono::Utc::now(),
        };
        let proj = HomeostaticProjectionDto {
            session: dummy_session,
            at: chrono::Utc::now(),
            horizon_seconds: 300,
            projected_state: state,
            hysteresis_gate_open: false,
        };
        assert_eq!(proj.horizon(), chrono::Duration::seconds(300));
    }
}
