//! Canonical state and patch types for the Agent OS.
//!
//! This module defines the protocol-level state model used for replay, UI sync,
//! and deterministic patch application, plus homeostasis types still used by
//! existing runtime components.

use crate::event::RiskLevel;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

// -----------------------------------------------------------------------------
// Canonical state + patch model
// -----------------------------------------------------------------------------

/// Reference to content-addressed blob payloads stored out-of-line.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlobRef {
    pub blob_id: String,
    pub content_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codec: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// Memory namespace. Stores projection pointers, not full payloads.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct MemoryNamespace {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary_ref: Option<BlobRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decisions_ref: Option<BlobRef>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub projections: BTreeMap<String, BlobRef>,
}

/// Canonical state model: one object with four namespaces.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct CanonicalState {
    pub session: Value,
    pub agent: Value,
    pub os: Value,
    pub memory: MemoryNamespace,
    /// Tombstones prevent accidental resurrection of forgotten paths.
    #[serde(skip)]
    tombstones: BTreeSet<String>,
}

impl Default for CanonicalState {
    fn default() -> Self {
        Self {
            session: Value::Object(Map::new()),
            agent: Value::Object(Map::new()),
            os: Value::Object(Map::new()),
            memory: MemoryNamespace::default(),
            tombstones: BTreeSet::new(),
        }
    }
}

/// Versioned state used by deterministic reducers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct VersionedCanonicalState {
    pub version: u64,
    #[serde(default)]
    pub state: CanonicalState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProvenanceRef {
    Event { event_id: String },
    Blob { blob_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum PatchOp {
    Set {
        path: String,
        value: Value,
    },
    Merge {
        path: String,
        object: Value,
    },
    Append {
        path: String,
        values: Vec<Value>,
    },
    Tombstone {
        path: String,
        reason: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        replaced_by: Option<String>,
    },
    SetRef {
        path: String,
        blob_ref: BlobRef,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct StatePatch {
    pub base_version: u64,
    #[serde(default)]
    pub ops: Vec<PatchOp>,
    #[serde(default)]
    pub provenance: Vec<ProvenanceRef>,
}

#[derive(Debug, Error, PartialEq)]
pub enum PatchApplyError {
    #[error("base version mismatch: expected {expected}, got {actual}")]
    BaseVersionMismatch { expected: u64, actual: u64 },
    #[error("invalid patch path: {0}")]
    InvalidPath(String),
    #[error("path is tombstoned and cannot be mutated: {0}")]
    Tombstoned(String),
    #[error("type conflict at path {path}: expected {expected}")]
    TypeConflict {
        path: String,
        expected: &'static str,
    },
}

impl VersionedCanonicalState {
    /// Apply a patch using deterministic reducer semantics.
    pub fn apply_patch(&mut self, patch: &StatePatch) -> Result<(), PatchApplyError> {
        if patch.base_version != self.version {
            return Err(PatchApplyError::BaseVersionMismatch {
                expected: self.version,
                actual: patch.base_version,
            });
        }

        for op in &patch.ops {
            self.state.apply_op(op)?;
        }

        self.version = self.version.saturating_add(1);
        Ok(())
    }
}

impl CanonicalState {
    fn apply_op(&mut self, op: &PatchOp) -> Result<(), PatchApplyError> {
        match op {
            PatchOp::Set { path, value } => {
                self.ensure_not_tombstoned(path)?;
                set_at_pointer(self, path, value.clone())
            }
            PatchOp::Merge { path, object } => {
                self.ensure_not_tombstoned(path)?;
                merge_at_pointer(self, path, object)
            }
            PatchOp::Append { path, values } => {
                self.ensure_not_tombstoned(path)?;
                append_at_pointer(self, path, values)
            }
            PatchOp::Tombstone {
                path,
                reason: _,
                replaced_by,
            } => {
                self.tombstones.insert(path.clone());
                if let Some(new_path) = replaced_by {
                    self.tombstones.insert(new_path.clone());
                }
                // Keep journal as source of truth; projection hides forgotten data.
                Ok(())
            }
            PatchOp::SetRef { path, blob_ref } => {
                self.ensure_not_tombstoned(path)?;
                set_at_pointer(
                    self,
                    path,
                    serde_json::to_value(blob_ref)
                        .map_err(|_| PatchApplyError::InvalidPath(path.clone()))?,
                )
            }
        }
    }

    fn ensure_not_tombstoned(&self, path: &str) -> Result<(), PatchApplyError> {
        if self
            .tombstones
            .iter()
            .any(|t| path == t || path.starts_with(&(t.to_string() + "/")))
        {
            return Err(PatchApplyError::Tombstoned(path.to_owned()));
        }
        Ok(())
    }
}

fn set_at_pointer(
    state: &mut CanonicalState,
    path: &str,
    value: Value,
) -> Result<(), PatchApplyError> {
    let mut root = serde_json::to_value(state.clone())
        .map_err(|_| PatchApplyError::InvalidPath(path.to_owned()))?;

    let parent_path = parent_pointer(path)?;
    let key = leaf_key(path)?;
    ensure_object_path(&mut root, parent_path)?;

    let parent = root
        .pointer_mut(parent_path)
        .ok_or_else(|| PatchApplyError::InvalidPath(path.to_owned()))?;

    match parent {
        Value::Object(map) => {
            map.insert(key.to_owned(), value);
        }
        _ => {
            return Err(PatchApplyError::TypeConflict {
                path: parent_path.to_owned(),
                expected: "object",
            });
        }
    }

    let tombstones = state.tombstones.clone();
    *state =
        serde_json::from_value(root).map_err(|_| PatchApplyError::InvalidPath(path.to_owned()))?;
    state.tombstones = tombstones;
    Ok(())
}

fn merge_at_pointer(
    state: &mut CanonicalState,
    path: &str,
    object: &Value,
) -> Result<(), PatchApplyError> {
    let mut root = serde_json::to_value(state.clone())
        .map_err(|_| PatchApplyError::InvalidPath(path.to_owned()))?;
    ensure_object_path(&mut root, path)?;

    let target = root
        .pointer_mut(path)
        .ok_or_else(|| PatchApplyError::InvalidPath(path.to_owned()))?;

    let patch_obj = object
        .as_object()
        .ok_or_else(|| PatchApplyError::TypeConflict {
            path: path.to_owned(),
            expected: "object",
        })?;

    let target_obj = target
        .as_object_mut()
        .ok_or_else(|| PatchApplyError::TypeConflict {
            path: path.to_owned(),
            expected: "object",
        })?;

    for (k, v) in patch_obj {
        target_obj.insert(k.clone(), v.clone());
    }

    let tombstones = state.tombstones.clone();
    *state =
        serde_json::from_value(root).map_err(|_| PatchApplyError::InvalidPath(path.to_owned()))?;
    state.tombstones = tombstones;
    Ok(())
}

fn append_at_pointer(
    state: &mut CanonicalState,
    path: &str,
    values: &[Value],
) -> Result<(), PatchApplyError> {
    let mut root = serde_json::to_value(state.clone())
        .map_err(|_| PatchApplyError::InvalidPath(path.to_owned()))?;
    ensure_array_path(&mut root, path)?;

    let target = root
        .pointer_mut(path)
        .ok_or_else(|| PatchApplyError::InvalidPath(path.to_owned()))?;

    let target_arr = target
        .as_array_mut()
        .ok_or_else(|| PatchApplyError::TypeConflict {
            path: path.to_owned(),
            expected: "array",
        })?;

    target_arr.extend(values.iter().cloned());

    let tombstones = state.tombstones.clone();
    *state =
        serde_json::from_value(root).map_err(|_| PatchApplyError::InvalidPath(path.to_owned()))?;
    state.tombstones = tombstones;
    Ok(())
}

fn ensure_object_path(root: &mut Value, path: &str) -> Result<(), PatchApplyError> {
    if path == "/" {
        return match root {
            Value::Object(_) => Ok(()),
            _ => Err(PatchApplyError::TypeConflict {
                path: "/".to_owned(),
                expected: "object",
            }),
        };
    }

    if !path.starts_with('/') {
        return Err(PatchApplyError::InvalidPath(path.to_owned()));
    }

    let mut current = root;
    for seg in path.trim_start_matches('/').split('/') {
        if seg.is_empty() {
            continue;
        }
        match current {
            Value::Object(map) => {
                current = map
                    .entry(seg.to_owned())
                    .or_insert_with(|| Value::Object(Map::new()));
            }
            _ => {
                return Err(PatchApplyError::TypeConflict {
                    path: path.to_owned(),
                    expected: "object",
                });
            }
        }
    }

    match current {
        Value::Object(_) => Ok(()),
        _ => Err(PatchApplyError::TypeConflict {
            path: path.to_owned(),
            expected: "object",
        }),
    }
}

fn ensure_array_path(root: &mut Value, path: &str) -> Result<(), PatchApplyError> {
    if !path.starts_with('/') {
        return Err(PatchApplyError::InvalidPath(path.to_owned()));
    }
    if root.pointer(path).is_some() {
        return Ok(());
    }

    let parent_path = parent_pointer(path)?;
    let key = leaf_key(path)?;
    ensure_object_path(root, parent_path)?;
    let parent = root
        .pointer_mut(parent_path)
        .ok_or_else(|| PatchApplyError::InvalidPath(path.to_owned()))?;
    match parent {
        Value::Object(map) => {
            map.entry(key.to_owned())
                .or_insert_with(|| Value::Array(Vec::new()));
            Ok(())
        }
        _ => Err(PatchApplyError::TypeConflict {
            path: parent_path.to_owned(),
            expected: "object",
        }),
    }
}

fn parent_pointer(path: &str) -> Result<&str, PatchApplyError> {
    if !path.starts_with('/') || path == "/" {
        return Err(PatchApplyError::InvalidPath(path.to_owned()));
    }
    path.rsplit_once('/')
        .map(|(p, _)| if p.is_empty() { "/" } else { p })
        .ok_or_else(|| PatchApplyError::InvalidPath(path.to_owned()))
}

fn leaf_key(path: &str) -> Result<&str, PatchApplyError> {
    if !path.starts_with('/') || path == "/" {
        return Err(PatchApplyError::InvalidPath(path.to_owned()));
    }
    path.rsplit('/')
        .next()
        .filter(|k| !k.is_empty())
        .ok_or_else(|| PatchApplyError::InvalidPath(path.to_owned()))
}

// -----------------------------------------------------------------------------
// Homeostasis model (kept for runtime compatibility)
// -----------------------------------------------------------------------------

/// The agent's internal health and resource state vector.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStateVector {
    pub progress: f32,
    pub uncertainty: f32,
    pub risk_level: RiskLevel,
    pub budget: BudgetState,
    pub error_streak: u32,
    pub context_pressure: f32,
    pub side_effect_pressure: f32,
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
    use serde_json::json;

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
    fn canonical_state_set_and_append() {
        let mut vs = VersionedCanonicalState::default();
        let patch = StatePatch {
            base_version: 0,
            ops: vec![
                PatchOp::Set {
                    path: "/session/files".to_owned(),
                    value: json!(["README.md"]),
                },
                PatchOp::Append {
                    path: "/session/files".to_owned(),
                    values: vec![json!("Cargo.toml")],
                },
            ],
            provenance: vec![ProvenanceRef::Event {
                event_id: "evt-1".to_owned(),
            }],
        };

        vs.apply_patch(&patch).unwrap();
        assert_eq!(vs.version, 1);
        assert_eq!(
            vs.state.session["files"],
            json!(["README.md", "Cargo.toml"])
        );
    }

    #[test]
    fn tombstone_blocks_resurrection() {
        let mut vs = VersionedCanonicalState::default();
        let first = StatePatch {
            base_version: 0,
            ops: vec![PatchOp::Tombstone {
                path: "/memory/projections/old".to_owned(),
                reason: "expired".to_owned(),
                replaced_by: None,
            }],
            provenance: vec![],
        };
        vs.apply_patch(&first).unwrap();

        let second = StatePatch {
            base_version: 1,
            ops: vec![PatchOp::Set {
                path: "/memory/projections/old".to_owned(),
                value: json!({"foo": "bar"}),
            }],
            provenance: vec![],
        };
        let err = vs.apply_patch(&second).unwrap_err();
        assert!(matches!(err, PatchApplyError::Tombstoned(_)));
    }
}
