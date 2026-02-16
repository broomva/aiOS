//! Homeostasis state types: agent vitals and budget tracking.
//!
//! These types represent the agent's internal health and resource state.
//! They are computed by the autonomic controller and stored as events.

use crate::event::RiskLevel;
use serde::{Deserialize, Serialize};

/// The agent's internal health and resource state vector.
///
/// Computed after each tool execution and during heartbeats.
/// Used by homeostasis controllers to make mode/gating decisions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStateVector {
    /// Task completion progress [0.0, 1.0].
    pub progress: f32,
    /// Epistemic uncertainty [0.0, 1.0]. Higher = less confidence.
    pub uncertainty: f32,
    /// Current risk assessment.
    pub risk_level: RiskLevel,
    /// Resource budget state.
    pub budget: BudgetState,
    /// Consecutive tool failures without success.
    pub error_streak: u32,
    /// Context window saturation [0.0, 1.0].
    pub context_pressure: f32,
    /// Pending uncommitted side effects [0.0, 1.0].
    pub side_effect_pressure: f32,
    /// Dependency on human input [0.0, 1.0].
    pub human_dependency: f32,
}

impl Default for AgentStateVector {
    fn default() -> Self {
        Self {
            progress: 0.0,
            uncertainty: 0.7,
            risk_level: RiskLevel::Low,
            budget: BudgetState::default(),
            error_streak: 0,
            context_pressure: 0.1,
            side_effect_pressure: 0.0,
            human_dependency: 0.0,
        }
    }
}

/// Resource budget tracking.
///
/// Decremented as the agent consumes resources. When any budget
/// reaches zero, the agent should enter a constrained mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetState {
    pub tokens_remaining: u64,
    pub time_remaining_ms: u64,
    pub cost_remaining_usd: f64,
    pub tool_calls_remaining: u32,
    pub error_budget_remaining: u32,
}

impl Default for BudgetState {
    fn default() -> Self {
        Self {
            tokens_remaining: 120_000,
            time_remaining_ms: 300_000,
            cost_remaining_usd: 5.0,
            tool_calls_remaining: 48,
            error_budget_remaining: 8,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_vector_default() {
        let sv = AgentStateVector::default();
        assert_eq!(sv.progress, 0.0);
        assert_eq!(sv.uncertainty, 0.7);
        assert_eq!(sv.error_streak, 0);
    }

    #[test]
    fn budget_default() {
        let b = BudgetState::default();
        assert_eq!(b.tokens_remaining, 120_000);
        assert_eq!(b.tool_calls_remaining, 48);
    }

    #[test]
    fn state_vector_serde_roundtrip() {
        let sv = AgentStateVector::default();
        let json = serde_json::to_string(&sv).unwrap();
        let back: AgentStateVector = serde_json::from_str(&json).unwrap();
        assert_eq!(back.progress, sv.progress);
        assert_eq!(back.error_streak, sv.error_streak);
    }
}
