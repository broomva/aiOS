//! Operating modes and gating profiles.
//!
//! The operating mode represents what the agent is currently doing.
//! The gating profile controls what the agent is allowed to do.

use crate::event::RiskLevel;
use serde::{Deserialize, Serialize};

/// The agent's current operating mode.
///
/// Mode transitions are driven by the homeostasis controller
/// based on the AgentStateVector.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatingMode {
    /// High uncertainty — gathering information, read-only tools preferred.
    Explore,
    /// Default productive mode — executing tools, making progress.
    #[default]
    Execute,
    /// High side-effect pressure — validating before committing.
    Verify,
    /// Error streak >= threshold — rollback, change strategy.
    Recover,
    /// Pending approvals or human input needed.
    AskHuman,
    /// Progress >= 98% or awaiting next signal.
    Sleep,
}

/// Dynamic constraints output by the homeostasis controller.
///
/// Enforced at the harness boundary in the runtime. Tighter than
/// static policy (which is the hard floor), gating provides
/// dynamic safety based on agent health state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatingProfile {
    /// Whether side effects (writes, deletes, network) are allowed.
    pub allow_side_effects: bool,
    /// Minimum risk level that requires human approval.
    pub require_approval_for_risk: RiskLevel,
    /// Maximum tool calls allowed per tick.
    pub max_tool_calls_per_tick: u32,
    /// Maximum file mutations allowed per tick.
    pub max_file_mutations_per_tick: u32,
    /// Whether network access is allowed.
    pub allow_network: bool,
    /// Whether shell execution is allowed.
    pub allow_shell: bool,
}

impl Default for GatingProfile {
    fn default() -> Self {
        Self {
            allow_side_effects: true,
            require_approval_for_risk: RiskLevel::High,
            max_tool_calls_per_tick: 10,
            max_file_mutations_per_tick: 5,
            allow_network: true,
            allow_shell: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operating_mode_serde_roundtrip() {
        for mode in [
            OperatingMode::Explore,
            OperatingMode::Execute,
            OperatingMode::Verify,
            OperatingMode::Recover,
            OperatingMode::AskHuman,
            OperatingMode::Sleep,
        ] {
            let json = serde_json::to_string(&mode).unwrap();
            let back: OperatingMode = serde_json::from_str(&json).unwrap();
            assert_eq!(mode, back);
        }
    }

    #[test]
    fn gating_profile_default() {
        let g = GatingProfile::default();
        assert!(g.allow_side_effects);
        assert!(g.allow_network);
        assert!(g.allow_shell);
        assert_eq!(g.max_tool_calls_per_tick, 10);
    }

    #[test]
    fn gating_profile_serde_roundtrip() {
        let g = GatingProfile::default();
        let json = serde_json::to_string(&g).unwrap();
        let back: GatingProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(back.max_tool_calls_per_tick, g.max_tool_calls_per_tick);
    }
}
