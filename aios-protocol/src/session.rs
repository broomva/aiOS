//! Session and checkpoint types.

use crate::ids::*;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Session manifest — describes a session's identity and configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionManifest {
    pub session_id: SessionId,
    pub owner: String,
    pub created_at: DateTime<Utc>,
    pub workspace_root: String,
    pub model_routing: ModelRouting,
    pub policy: serde_json::Value,
}

/// LLM model routing configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRouting {
    pub primary_model: String,
    pub fallback_models: Vec<String>,
    pub temperature: f32,
}

impl Default for ModelRouting {
    fn default() -> Self {
        Self {
            primary_model: "claude-sonnet-4-5-20250929".to_owned(),
            fallback_models: vec!["gpt-4.1".to_owned()],
            temperature: 0.2,
        }
    }
}

/// Branch metadata within a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchInfo {
    pub branch_id: BranchId,
    pub parent_branch: Option<BranchId>,
    pub fork_sequence: u64,
    pub head_sequence: u64,
    pub merged_into: Option<BranchId>,
}

/// Result of merging two branches.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchMergeResult {
    pub source_branch: BranchId,
    pub target_branch: BranchId,
    pub source_head_sequence: u64,
    pub target_head_sequence: u64,
}

/// Checkpoint manifest — a snapshot of state at a specific point.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointManifest {
    pub checkpoint_id: CheckpointId,
    pub session_id: SessionId,
    pub branch_id: BranchId,
    pub created_at: DateTime<Utc>,
    pub event_sequence: u64,
    pub state_hash: String,
    pub note: String,
}

/// Request to create a new session.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct CreateSessionRequest {
    pub owner: String,
    #[serde(default)]
    pub policy: crate::policy::PolicySet,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_session: Option<SessionId>,
    #[serde(default)]
    pub labels: Vec<String>,
}

/// Filter for listing sessions.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SessionFilter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

/// Input for a single agent tick.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TickInput {
    pub objective: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposed_tool: Option<crate::tool::ToolCall>,
    #[serde(default)]
    pub max_iterations: Option<u32>,
}

/// Output from a single agent tick.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TickOutput {
    pub session_id: SessionId,
    pub iteration: u32,
    pub stop_reason: crate::ports::ModelStopReason,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_answer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<crate::event::TokenUsage>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_routing_default() {
        let mr = ModelRouting::default();
        assert!(mr.primary_model.contains("claude"));
        assert_eq!(mr.temperature, 0.2);
    }

    #[test]
    fn session_manifest_serde_roundtrip() {
        let manifest = SessionManifest {
            session_id: SessionId::from_string("S1"),
            owner: "test".into(),
            created_at: Utc::now(),
            workspace_root: "/tmp/test".into(),
            model_routing: ModelRouting::default(),
            policy: serde_json::json!({}),
        };
        let json = serde_json::to_string(&manifest).unwrap();
        let back: SessionManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.session_id.as_str(), "S1");
    }
}
