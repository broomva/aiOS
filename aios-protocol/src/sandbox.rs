//! Canonical sandbox types for the Agent OS.
//!
//! These types define the shared vocabulary for sandbox isolation across
//! all projects (Arcan, Lago, Praxis). Implementations live in their
//! respective crates; this module provides only the contract.

use serde::{Deserialize, Serialize};

/// Sandbox isolation tiers, ordered from least to most isolated.
///
/// Derives `PartialOrd`/`Ord` so comparisons like `tier >= SandboxTier::Process`
/// work naturally for policy enforcement. Variant order is load-bearing:
/// the derived `Ord` ranks tiers by declaration order, so new tiers MUST be
/// appended (never inserted).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum SandboxTier {
    /// No isolation — direct host access.
    #[default]
    None,
    /// Basic restrictions (e.g. seccomp, pledge).
    Basic,
    /// Process-level isolation (e.g. bubblewrap, firejail).
    Process,
    /// Full container isolation (e.g. Apple Containers, Docker).
    Container,
    /// Hardware-isolated micro-VM (e.g. Cloud Hypervisor, Firecracker).
    ///
    /// The strongest tier on the scale — separate kernel, separate address
    /// space, independent resource accounting. Used by `lifed` as the
    /// default tier for agent VMs emitted by the kernel daemon.
    ///
    /// Serialized as `"micro_vm"` (explicit `rename` override — serde's
    /// default `snake_case` conversion would emit `"micro_v_m"` for
    /// PascalCase `MicroVM`, which reads poorly and does not match the
    /// upstream spec wire shape).
    #[serde(rename = "micro_vm")]
    MicroVM,
}

/// Resource limits for sandboxed command execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SandboxLimits {
    /// Maximum wall-clock execution time in seconds.
    pub max_runtime_secs: u64,
    /// Maximum bytes for stdout/stderr output.
    pub max_output_bytes: usize,
    /// Maximum memory in megabytes (optional, not always enforced).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_memory_mb: Option<u64>,
}

impl Default for SandboxLimits {
    fn default() -> Self {
        Self {
            max_runtime_secs: 30,
            max_output_bytes: 64 * 1024,
            max_memory_mb: None,
        }
    }
}

/// Network access policy for sandboxed execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(tag = "policy", rename_all = "snake_case")]
pub enum NetworkPolicy {
    /// No network access allowed.
    #[default]
    Disabled,
    /// Unrestricted network access.
    AllowAll,
    /// Network access limited to specific hosts.
    AllowList {
        #[serde(default)]
        hosts: Vec<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── SandboxTier tests ──

    #[test]
    fn tier_ordering() {
        assert!(SandboxTier::None < SandboxTier::Basic);
        assert!(SandboxTier::Basic < SandboxTier::Process);
        assert!(SandboxTier::Process < SandboxTier::Container);
    }

    #[test]
    fn tier_default_is_none() {
        assert_eq!(SandboxTier::default(), SandboxTier::None);
    }

    #[test]
    fn tier_serde_roundtrip() {
        for tier in [
            SandboxTier::None,
            SandboxTier::Basic,
            SandboxTier::Process,
            SandboxTier::Container,
            SandboxTier::MicroVM,
        ] {
            let json = serde_json::to_string(&tier).unwrap();
            let back: SandboxTier = serde_json::from_str(&json).unwrap();
            assert_eq!(back, tier);
        }
        assert_eq!(
            serde_json::to_string(&SandboxTier::None).unwrap(),
            "\"none\""
        );
        assert_eq!(
            serde_json::to_string(&SandboxTier::Container).unwrap(),
            "\"container\""
        );
    }

    #[test]
    fn microvm_is_highest_tier() {
        // Lock in the ordering contract: MicroVM must sit above every other
        // tier. Downstream policy code relies on this to express "isolate
        // at least as strongly as X" comparisons.
        assert!(SandboxTier::MicroVM > SandboxTier::Container);
        assert!(SandboxTier::Container > SandboxTier::Process);
        assert!(SandboxTier::Process > SandboxTier::Basic);
        assert!(SandboxTier::Basic > SandboxTier::None);
        // And lock in the serde name — `rename_all = "snake_case"` turns
        // PascalCase variants into snake_case on the wire.
        assert_eq!(
            serde_json::to_string(&SandboxTier::MicroVM).unwrap(),
            "\"micro_vm\""
        );
    }

    #[test]
    fn tier_ge_comparison_for_policy() {
        let required = SandboxTier::Process;
        assert!(SandboxTier::Process >= required);
        assert!(SandboxTier::Container >= required);
        assert!(SandboxTier::Basic < required);
        assert!(SandboxTier::None < required);
    }

    // ── SandboxLimits tests ──

    #[test]
    fn limits_default() {
        let limits = SandboxLimits::default();
        assert_eq!(limits.max_runtime_secs, 30);
        assert_eq!(limits.max_output_bytes, 64 * 1024);
        assert!(limits.max_memory_mb.is_none());
    }

    #[test]
    fn limits_serde_roundtrip() {
        let limits = SandboxLimits {
            max_runtime_secs: 60,
            max_output_bytes: 128 * 1024,
            max_memory_mb: Some(512),
        };
        let json = serde_json::to_string(&limits).unwrap();
        let back: SandboxLimits = serde_json::from_str(&json).unwrap();
        assert_eq!(limits, back);
    }

    #[test]
    fn limits_omits_none_memory() {
        let limits = SandboxLimits::default();
        let json = serde_json::to_string(&limits).unwrap();
        assert!(!json.contains("max_memory_mb"));
    }

    // ── NetworkPolicy tests ──

    #[test]
    fn network_policy_default_is_disabled() {
        assert_eq!(NetworkPolicy::default(), NetworkPolicy::Disabled);
    }

    #[test]
    fn network_policy_disabled_serde() {
        let policy = NetworkPolicy::Disabled;
        let json = serde_json::to_string(&policy).unwrap();
        assert!(json.contains("\"policy\":\"disabled\""));
        let back: NetworkPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(policy, back);
    }

    #[test]
    fn network_policy_allow_all_serde() {
        let policy = NetworkPolicy::AllowAll;
        let json = serde_json::to_string(&policy).unwrap();
        let back: NetworkPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(policy, back);
    }

    #[test]
    fn network_policy_allow_list_serde() {
        let policy = NetworkPolicy::AllowList {
            hosts: vec!["api.anthropic.com".into(), "api.openai.com".into()],
        };
        let json = serde_json::to_string(&policy).unwrap();
        assert!(json.contains("api.anthropic.com"));
        let back: NetworkPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(policy, back);
    }

    #[test]
    fn network_policy_allow_list_empty_hosts() {
        let json = r#"{"policy":"allow_list"}"#;
        let policy: NetworkPolicy = serde_json::from_str(json).unwrap();
        match policy {
            NetworkPolicy::AllowList { hosts } => assert!(hosts.is_empty()),
            _ => panic!("expected AllowList"),
        }
    }
}
