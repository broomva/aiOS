//! Memory types: soul profile, observations, and provenance.

use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// Memory scope for scoped storage and retrieval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryScope {
    Session,
    User,
    Agent,
    Org,
}

/// Durable agent identity and preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoulProfile {
    pub name: String,
    pub mission: String,
    pub preferences: IndexMap<String, String>,
    pub updated_at: DateTime<Utc>,
}

impl Default for SoulProfile {
    fn default() -> Self {
        Self {
            name: "Agent OS agent".to_owned(),
            mission: "Run tool-mediated work safely and reproducibly".to_owned(),
            preferences: IndexMap::new(),
            updated_at: Utc::now(),
        }
    }
}

/// An extracted observation with provenance linking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub observation_id: uuid::Uuid,
    pub created_at: DateTime<Utc>,
    pub text: String,
    pub tags: Vec<String>,
    pub provenance: Provenance,
}

/// Links an observation back to its source events and files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provenance {
    /// First event sequence in the source range.
    pub event_start: u64,
    /// Last event sequence in the source range.
    pub event_end: u64,
    /// Files referenced with their content hashes.
    pub files: Vec<FileProvenance>,
}

/// A file reference with content hash for provenance tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileProvenance {
    pub path: String,
    pub sha256: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_scope_ordering() {
        assert!(MemoryScope::Session < MemoryScope::User);
        assert!(MemoryScope::User < MemoryScope::Agent);
        assert!(MemoryScope::Agent < MemoryScope::Org);
    }

    #[test]
    fn memory_scope_serde_roundtrip() {
        for scope in [
            MemoryScope::Session,
            MemoryScope::User,
            MemoryScope::Agent,
            MemoryScope::Org,
        ] {
            let json = serde_json::to_string(&scope).unwrap();
            let back: MemoryScope = serde_json::from_str(&json).unwrap();
            assert_eq!(scope, back);
        }
    }

    #[test]
    fn soul_profile_default() {
        let soul = SoulProfile::default();
        assert_eq!(soul.name, "Agent OS agent");
        assert!(soul.preferences.is_empty());
    }
}
