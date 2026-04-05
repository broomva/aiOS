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
