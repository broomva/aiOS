//! World state DTOs (served by opsisd).

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct WorldId(pub String);

#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct WorldSnapshot {
    pub world: WorldId,
    pub version: u64,
    pub state: serde_json::Value,
    pub taken_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct WorldMutation {
    pub ops: Vec<WorldMutationOp>,
    pub issued_by: crate::ids::AgentId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub causation_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct WorldMutationOp {
    pub path: String,
    pub op: WorldOpKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum WorldOpKind {
    Set,
    Remove,
    Merge,
    Patch,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorldVersion {
    pub world: WorldId,
    pub version: u64,
    pub committed_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct WorldEvent {
    pub world: WorldId,
    pub version: u64,
    pub op: WorldMutationOp,
    pub occurred_at: chrono::DateTime<chrono::Utc>,
}
