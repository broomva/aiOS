//! Agent identity provider trait — the interface between the kernel contract
//! and identity implementations (Anima or basic).

use crate::ids::AgentId;
use crate::memory::SoulProfile;

/// Trait for providing agent identity to the runtime.
///
/// Default implementation ([`BasicIdentity`]) provides name/mission only.
/// Anima implementation adds crypto identity, DID, policy enforcement.
pub trait AgentIdentityProvider: Send + Sync + std::fmt::Debug {
    /// Agent's unique identifier.
    fn agent_id(&self) -> &AgentId;

    /// Agent's soul profile (name, mission, preferences).
    fn soul_profile(&self) -> &SoulProfile;

    /// Agent's DID (did:key:z6Mk...). None for basic identity.
    fn did(&self) -> Option<&str> {
        None
    }

    /// Sign a JWT with the agent's identity key. None if no crypto identity.
    fn sign_jwt(&self, _audience: &str, _ttl_secs: u64) -> Option<String> {
        None
    }

    /// List of granted capabilities. Empty = unrestricted.
    fn capabilities(&self) -> &[String] {
        &[]
    }

    /// Current economic mode from belief state.
    fn economic_mode(&self) -> &str {
        "sovereign"
    }

    /// Check if a specific action is allowed by the agent's policy.
    fn policy_allows(&self, _action: &str) -> bool {
        true
    }

    /// Build a persona block for the system prompt.
    fn persona_block(&self) -> String {
        let soul = self.soul_profile();
        let mut block = format!("You are {} — {}.", soul.name, soul.mission);
        if let Some(did) = self.did() {
            block.push_str(&format!("\nIdentity: {did}"));
        }
        let caps = self.capabilities();
        if !caps.is_empty() {
            block.push_str(&format!("\nCapabilities: {}", caps.join(", ")));
        }
        block.push_str(&format!("\nEconomic mode: {}", self.economic_mode()));
        block
    }
}

/// Basic identity provider for open-source usage (no crypto, no policy).
#[derive(Debug, Clone, Default)]
pub struct BasicIdentity {
    pub agent_id: AgentId,
    pub soul: SoulProfile,
}

impl BasicIdentity {
    pub fn new(name: impl Into<String>, mission: impl Into<String>) -> Self {
        Self {
            agent_id: AgentId::default(),
            soul: SoulProfile {
                name: name.into(),
                mission: mission.into(),
                ..Default::default()
            },
        }
    }

    pub fn with_id(mut self, id: AgentId) -> Self {
        self.agent_id = id;
        self
    }
}

impl AgentIdentityProvider for BasicIdentity {
    fn agent_id(&self) -> &AgentId {
        &self.agent_id
    }

    fn soul_profile(&self) -> &SoulProfile {
        &self.soul
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_identity_default() {
        let id = BasicIdentity::default();
        assert_eq!(id.soul_profile().name, "Agent OS agent");
        assert_eq!(
            id.soul_profile().mission,
            "Run tool-mediated work safely and reproducibly"
        );
    }

    #[test]
    fn basic_identity_custom_name_mission() {
        let id = BasicIdentity::new("Arcan Prime", "Runtime cognition for the Agent OS");
        assert_eq!(id.soul_profile().name, "Arcan Prime");
        assert_eq!(
            id.soul_profile().mission,
            "Runtime cognition for the Agent OS"
        );
    }

    #[test]
    fn basic_identity_with_id() {
        let custom_id = AgentId::from_string("agt_custom_001");
        let id = BasicIdentity::new("test", "test mission").with_id(custom_id.clone());
        assert_eq!(id.agent_id(), &custom_id);
    }

    #[test]
    fn trait_defaults_no_did() {
        let id = BasicIdentity::default();
        assert!(id.did().is_none());
    }

    #[test]
    fn trait_defaults_no_jwt() {
        let id = BasicIdentity::default();
        assert!(id.sign_jwt("aud", 3600).is_none());
    }

    #[test]
    fn trait_defaults_empty_capabilities() {
        let id = BasicIdentity::default();
        assert!(id.capabilities().is_empty());
    }

    #[test]
    fn trait_defaults_sovereign_mode() {
        let id = BasicIdentity::default();
        assert_eq!(id.economic_mode(), "sovereign");
    }

    #[test]
    fn trait_defaults_policy_allows_all() {
        let id = BasicIdentity::default();
        assert!(id.policy_allows("any_action"));
        assert!(id.policy_allows("fs:write"));
    }

    #[test]
    fn persona_block_basic() {
        let id = BasicIdentity::new("Arcan", "Agent runtime");
        let block = id.persona_block();
        assert!(block.contains("You are Arcan"));
        assert!(block.contains("Agent runtime"));
        assert!(block.contains("Economic mode: sovereign"));
        // No DID or capabilities for basic identity
        assert!(!block.contains("Identity:"));
        assert!(!block.contains("Capabilities:"));
    }

    #[test]
    fn persona_block_with_did_and_caps() {
        // Test with a custom implementation that provides DID and capabilities
        #[derive(Debug)]
        struct RichIdentity {
            id: AgentId,
            soul: SoulProfile,
            did: String,
            caps: Vec<String>,
        }

        impl AgentIdentityProvider for RichIdentity {
            fn agent_id(&self) -> &AgentId {
                &self.id
            }
            fn soul_profile(&self) -> &SoulProfile {
                &self.soul
            }
            fn did(&self) -> Option<&str> {
                Some(&self.did)
            }
            fn capabilities(&self) -> &[String] {
                &self.caps
            }
            fn economic_mode(&self) -> &str {
                "hustle"
            }
        }

        let rich = RichIdentity {
            id: AgentId::default(),
            soul: SoulProfile {
                name: "Agent X".into(),
                mission: "Do things".into(),
                ..Default::default()
            },
            did: "did:key:z6MkTest123".into(),
            caps: vec!["chat:send".into(), "fs:read".into()],
        };

        let block = rich.persona_block();
        assert!(block.contains("You are Agent X"));
        assert!(block.contains("Identity: did:key:z6MkTest123"));
        assert!(block.contains("Capabilities: chat:send, fs:read"));
        assert!(block.contains("Economic mode: hustle"));
    }
}
