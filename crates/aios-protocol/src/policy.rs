//! Policy types: capabilities, policy sets, and evaluation results.

use serde::{Deserialize, Serialize};

/// A capability token representing a specific permission.
///
/// Capabilities are pattern-based strings like `"fs:read:/session/**"`.
/// They support glob matching for flexible policy evaluation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Capability(pub String);

impl Capability {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn fs_read(glob: &str) -> Self {
        Self(format!("fs:read:{glob}"))
    }

    pub fn fs_write(glob: &str) -> Self {
        Self(format!("fs:write:{glob}"))
    }

    pub fn net_egress(host: &str) -> Self {
        Self(format!("net:egress:{host}"))
    }

    pub fn exec(command: &str) -> Self {
        Self(format!("exec:cmd:{command}"))
    }

    pub fn secrets(scope: &str) -> Self {
        Self(format!("secrets:read:{scope}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A set of policy rules governing agent capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicySet {
    pub allow_capabilities: Vec<Capability>,
    pub gate_capabilities: Vec<Capability>,
    pub max_tool_runtime_secs: u64,
    pub max_events_per_turn: u64,
}

impl Default for PolicySet {
    fn default() -> Self {
        Self {
            allow_capabilities: vec![
                Capability::fs_read("/session/**"),
                Capability::fs_write("/session/artifacts/**"),
                Capability::exec("git"),
            ],
            gate_capabilities: vec![Capability::new("payments:initiate")],
            max_tool_runtime_secs: 30,
            max_events_per_turn: 256,
        }
    }
}

/// Result of evaluating capabilities against a policy set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyEvaluation {
    pub allowed: Vec<Capability>,
    pub requires_approval: Vec<Capability>,
    pub denied: Vec<Capability>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_factory_methods() {
        assert_eq!(Capability::fs_read("/tmp").as_str(), "fs:read:/tmp");
        assert_eq!(Capability::fs_write("/out").as_str(), "fs:write:/out");
        assert_eq!(
            Capability::net_egress("api.com").as_str(),
            "net:egress:api.com"
        );
        assert_eq!(Capability::exec("git").as_str(), "exec:cmd:git");
        assert_eq!(Capability::secrets("prod").as_str(), "secrets:read:prod");
    }

    #[test]
    fn policy_set_default() {
        let ps = PolicySet::default();
        assert_eq!(ps.allow_capabilities.len(), 3);
        assert_eq!(ps.gate_capabilities.len(), 1);
        assert_eq!(ps.max_tool_runtime_secs, 30);
    }

    #[test]
    fn capability_serde_roundtrip() {
        let cap = Capability::fs_read("/session/**");
        let json = serde_json::to_string(&cap).unwrap();
        let back: Capability = serde_json::from_str(&json).unwrap();
        assert_eq!(cap, back);
    }
}
