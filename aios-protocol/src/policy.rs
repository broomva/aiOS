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

impl PolicySet {
    /// Heavily restricted — anonymous public users. No side-effecting capabilities.
    ///
    /// Shell execution (`exec:cmd:*`) is NOT gated (approval queue) — it is
    /// absent from both `allow_capabilities` and `gate_capabilities`, so the
    /// policy engine immediately **denies** any bash/shell tool call without
    /// creating an approval ticket. BRO-216.
    ///
    /// 5 events/turn, 30s tool runtime.
    pub fn anonymous() -> Self {
        Self {
            allow_capabilities: vec![Capability::new("fs:read:/session/**")],
            // exec:cmd:* removed — falls through to denied by StaticPolicyEngine.
            gate_capabilities: vec![
                Capability::new("fs:write:**"),
                Capability::new("net:egress:*"),
                Capability::new("secrets:read:*"),
            ],
            max_tool_runtime_secs: 30,
            max_events_per_turn: 5,
        }
    }

    /// Read + network + limited shell — authenticated free tier users.
    ///
    /// Shell execution is restricted to a safe read-only whitelist; unlisted
    /// commands are denied immediately (not gated). BRO-216.
    ///
    /// 15 events/turn, 30s tool runtime.
    pub fn free() -> Self {
        Self {
            allow_capabilities: vec![
                Capability::new("fs:read:/session/**"),
                Capability::new("net:egress:*"),
                // Shell whitelist — safe read-only commands only.
                Capability::new("exec:cmd:cat"),
                Capability::new("exec:cmd:ls"),
                Capability::new("exec:cmd:echo"),
                Capability::new("exec:cmd:grep"),
                Capability::new("exec:cmd:jq"),
                Capability::new("exec:cmd:python3"),
                Capability::new("exec:cmd:find"),
                Capability::new("exec:cmd:head"),
                Capability::new("exec:cmd:tail"),
                Capability::new("exec:cmd:sort"),
                Capability::new("exec:cmd:wc"),
            ],
            // exec:cmd:* removed — unlisted exec commands fall through to denied.
            gate_capabilities: vec![
                Capability::new("fs:write:**"),
                Capability::new("secrets:read:*"),
            ],
            max_tool_runtime_secs: 30,
            max_events_per_turn: 15,
        }
    }

    /// Full access — authenticated Pro subscribers.
    /// 50 events/turn, 60s tool runtime.
    pub fn pro() -> Self {
        Self {
            allow_capabilities: vec![Capability::new("*")],
            gate_capabilities: vec![],
            max_tool_runtime_secs: 60,
            max_events_per_turn: 50,
        }
    }

    /// Fully permissive — Enterprise tenants (custom overrides applied separately).
    /// 200 events/turn, 120s tool runtime.
    pub fn enterprise() -> Self {
        Self {
            allow_capabilities: vec![Capability::new("*")],
            gate_capabilities: vec![],
            max_tool_runtime_secs: 120,
            max_events_per_turn: 200,
        }
    }
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

/// Subscription tier for a user or tenant.
///
/// Controls session TTLs, rate limits, and capability grants across the
/// Agent OS (Arcan session store, Lago metering, Praxis tool limits).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionTier {
    /// Unauthenticated public access — zero persistence, minimal capabilities.
    #[default]
    Anonymous,
    /// Authenticated free tier — 7-day session TTL.
    Free,
    /// Authenticated Pro subscriber — 90-day session TTL, full tool access.
    Pro,
    /// Enterprise tenant — no session expiry, custom capability overrides.
    Enterprise,
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

    #[test]
    fn policy_set_anonymous() {
        let ps = PolicySet::anonymous();
        assert_eq!(ps.allow_capabilities.len(), 1);
        assert_eq!(ps.allow_capabilities[0].as_str(), "fs:read:/session/**");
        // exec:cmd:* must NOT be in gate_capabilities — it must be denied outright (BRO-216).
        assert_eq!(ps.gate_capabilities.len(), 3);
        assert_eq!(ps.max_tool_runtime_secs, 30);
        assert_eq!(ps.max_events_per_turn, 5);
        // anonymous: exec is in neither allow nor gate → immediately denied
        let exec_cap = Capability::new("exec:cmd:*");
        assert!(!ps.allow_capabilities.contains(&exec_cap));
        assert!(!ps.gate_capabilities.contains(&exec_cap));
    }

    #[test]
    fn policy_set_free() {
        let ps = PolicySet::free();
        // allow: session read + net egress + 11 whitelisted exec commands
        assert_eq!(ps.allow_capabilities.len(), 13);
        // gate: fs:write + secrets (exec removed — unlisted exec → denied)
        assert_eq!(ps.gate_capabilities.len(), 2);
        assert_eq!(ps.max_tool_runtime_secs, 30);
        assert_eq!(ps.max_events_per_turn, 15);
        // free allows net egress
        assert!(
            ps.allow_capabilities
                .contains(&Capability::new("net:egress:*"))
        );
        // free has whitelisted exec commands
        assert!(
            ps.allow_capabilities
                .contains(&Capability::new("exec:cmd:cat"))
        );
        assert!(
            ps.allow_capabilities
                .contains(&Capability::new("exec:cmd:grep"))
        );
        // exec:cmd:* wildcard is NOT in gate (unlisted commands → denied immediately)
        assert!(
            !ps.gate_capabilities
                .contains(&Capability::new("exec:cmd:*"))
        );
    }

    #[test]
    fn policy_set_pro() {
        let ps = PolicySet::pro();
        assert_eq!(ps.allow_capabilities.len(), 1);
        assert_eq!(ps.allow_capabilities[0].as_str(), "*");
        assert_eq!(ps.gate_capabilities.len(), 0);
        assert_eq!(ps.max_tool_runtime_secs, 60);
        assert_eq!(ps.max_events_per_turn, 50);
        // pro allows all via wildcard
        assert!(ps.allow_capabilities.contains(&Capability::new("*")));
    }

    #[test]
    fn policy_set_enterprise() {
        let ps = PolicySet::enterprise();
        assert_eq!(ps.allow_capabilities.len(), 1);
        assert_eq!(ps.allow_capabilities[0].as_str(), "*");
        assert_eq!(ps.gate_capabilities.len(), 0);
        assert_eq!(ps.max_tool_runtime_secs, 120);
        assert_eq!(ps.max_events_per_turn, 200);
        // enterprise allows all via wildcard
        assert!(ps.allow_capabilities.contains(&Capability::new("*")));
    }
}
