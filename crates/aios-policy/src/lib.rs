use std::collections::HashMap;
use std::sync::Arc;

use aios_protocol::{Capability, PolicySet, SessionId};
use async_trait::async_trait;
use indexmap::IndexSet;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyEvaluation {
    pub allowed: Vec<Capability>,
    pub requires_approval: Vec<Capability>,
    pub denied: Vec<Capability>,
}

impl PolicyEvaluation {
    pub fn is_allowed_now(&self) -> bool {
        self.denied.is_empty() && self.requires_approval.is_empty()
    }
}

#[derive(Debug, Clone)]
pub struct ApprovalTicket {
    pub approval_id: Uuid,
    pub session_id: SessionId,
    pub capability: Capability,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct ApprovalResolution {
    pub approved: bool,
    pub actor: String,
}

#[async_trait]
pub trait PolicyEngine: Send + Sync {
    async fn evaluate_capabilities(
        &self,
        session_id: SessionId,
        requested: &[Capability],
    ) -> PolicyEvaluation;
}

#[derive(Debug, Clone)]
pub struct StaticPolicyEngine {
    allow: Vec<Capability>,
    gate: Vec<Capability>,
}

impl StaticPolicyEngine {
    pub fn from_policy_set(policy: &PolicySet) -> Self {
        Self {
            allow: policy.allow_capabilities.clone(),
            gate: policy.gate_capabilities.clone(),
        }
    }

    fn matches(pattern: &str, actual: &str) -> bool {
        if pattern.ends_with('*') {
            let prefix = pattern.trim_end_matches('*');
            prefix.is_empty() || actual.starts_with(prefix)
        } else {
            pattern == actual
        }
    }

    fn set_contains(set: &[Capability], requested: &Capability) -> bool {
        set.iter()
            .any(|candidate| Self::matches(candidate.as_str(), requested.as_str()))
    }
}

#[async_trait]
impl PolicyEngine for StaticPolicyEngine {
    async fn evaluate_capabilities(
        &self,
        _session_id: SessionId,
        requested: &[Capability],
    ) -> PolicyEvaluation {
        let mut allowed = IndexSet::new();
        let mut requires_approval = IndexSet::new();
        let mut denied = IndexSet::new();

        for capability in requested {
            if Self::set_contains(&self.gate, capability) {
                requires_approval.insert(capability.clone());
                continue;
            }

            if Self::set_contains(&self.allow, capability) {
                allowed.insert(capability.clone());
                continue;
            }

            denied.insert(capability.clone());
        }

        PolicyEvaluation {
            allowed: allowed.into_iter().collect(),
            requires_approval: requires_approval.into_iter().collect(),
            denied: denied.into_iter().collect(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SessionPolicyEngine {
    default: StaticPolicyEngine,
    overrides: Arc<RwLock<HashMap<String, StaticPolicyEngine>>>,
}

impl SessionPolicyEngine {
    pub fn new(default_policy: PolicySet) -> Self {
        Self {
            default: StaticPolicyEngine::from_policy_set(&default_policy),
            overrides: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn set_policy(&self, session_id: &SessionId, policy: &PolicySet) {
        self.overrides.write().await.insert(
            session_id.as_str().to_owned(),
            StaticPolicyEngine::from_policy_set(policy),
        );
    }
}

#[async_trait]
impl PolicyEngine for SessionPolicyEngine {
    async fn evaluate_capabilities(
        &self,
        session_id: SessionId,
        requested: &[Capability],
    ) -> PolicyEvaluation {
        if let Some(engine) = self
            .overrides
            .read()
            .await
            .get(session_id.as_str())
            .cloned()
        {
            engine.evaluate_capabilities(session_id, requested).await
        } else {
            self.default
                .evaluate_capabilities(session_id, requested)
                .await
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct ApprovalQueue {
    pending: Arc<RwLock<HashMap<Uuid, ApprovalTicket>>>,
    resolved: Arc<RwLock<HashMap<Uuid, ApprovalResolution>>>,
}

impl ApprovalQueue {
    pub async fn enqueue(
        &self,
        session_id: SessionId,
        capability: Capability,
        reason: impl Into<String>,
    ) -> ApprovalTicket {
        let approval_id = Uuid::new_v4();
        let ticket = ApprovalTicket {
            approval_id,
            session_id,
            capability,
            reason: reason.into(),
        };
        self.pending
            .write()
            .await
            .insert(approval_id, ticket.clone());
        ticket
    }

    pub async fn resolve(
        &self,
        approval_id: Uuid,
        approved: bool,
        actor: impl Into<String>,
    ) -> Option<ApprovalResolution> {
        if self.pending.write().await.remove(&approval_id).is_some() {
            let resolution = ApprovalResolution {
                approved,
                actor: actor.into(),
            };
            self.resolved
                .write()
                .await
                .insert(approval_id, resolution.clone());
            Some(resolution)
        } else {
            None
        }
    }

    pub async fn pending_for_session(&self, session_id: &SessionId) -> Vec<ApprovalTicket> {
        self.pending
            .read()
            .await
            .values()
            .filter(|ticket| ticket.session_id == *session_id)
            .cloned()
            .collect()
    }

    pub async fn resolution(&self, approval_id: Uuid) -> Option<ApprovalResolution> {
        self.resolved.read().await.get(&approval_id).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn static_policy_engine_routes_allow_gate_and_deny() {
        let policy = PolicySet {
            allow_capabilities: vec![Capability::exec("*")],
            gate_capabilities: vec![Capability::new("payments:initiate")],
            max_tool_runtime_secs: 10,
            max_events_per_turn: 10,
        };

        let engine = StaticPolicyEngine::from_policy_set(&policy);
        let session_id = SessionId::default();

        let allowed_eval = engine
            .evaluate_capabilities(session_id.clone(), &[Capability::exec("echo")])
            .await;
        assert_eq!(allowed_eval.allowed.len(), 1);
        assert!(allowed_eval.requires_approval.is_empty());
        assert!(allowed_eval.denied.is_empty());

        let gated_eval = engine
            .evaluate_capabilities(session_id.clone(), &[Capability::new("payments:initiate")])
            .await;
        assert_eq!(gated_eval.requires_approval.len(), 1);
        assert!(gated_eval.allowed.is_empty());
        assert!(gated_eval.denied.is_empty());

        let denied_eval = engine
            .evaluate_capabilities(session_id, &[Capability::net_egress("example.com")])
            .await;
        assert_eq!(denied_eval.denied.len(), 1);
        assert!(denied_eval.allowed.is_empty());
        assert!(denied_eval.requires_approval.is_empty());
    }

    #[tokio::test]
    async fn session_policy_engine_applies_per_session_override() {
        let default_policy = PolicySet {
            allow_capabilities: vec![Capability::fs_read("/session/**")],
            gate_capabilities: vec![],
            max_tool_runtime_secs: 10,
            max_events_per_turn: 10,
        };

        let engine = SessionPolicyEngine::new(default_policy);
        let base_session = SessionId::default();
        let override_session = SessionId::default();

        let override_policy = PolicySet {
            allow_capabilities: vec![Capability::exec("*")],
            gate_capabilities: vec![],
            max_tool_runtime_secs: 10,
            max_events_per_turn: 10,
        };
        engine.set_policy(&override_session, &override_policy).await;

        let base_eval = engine
            .evaluate_capabilities(base_session, &[Capability::exec("echo")])
            .await;
        assert_eq!(base_eval.denied.len(), 1);

        let override_eval = engine
            .evaluate_capabilities(override_session, &[Capability::exec("echo")])
            .await;
        assert_eq!(override_eval.allowed.len(), 1);
        assert!(override_eval.denied.is_empty());
    }
}
