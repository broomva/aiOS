//! Canonical ID types for the Agent OS.
//!
//! IDs are opaque String wrappers (serde-transparent) to support both ULID
//! (Lago-style, 26-char sortable) and UUID (aiOS-style) generation strategies.
//! Consumers choose their preferred generation; the kernel only requires String.

use serde::{Deserialize, Serialize};
use std::fmt;

macro_rules! typed_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Create from any string value.
            pub fn from_string(s: impl Into<String>) -> Self {
                Self(s.into())
            }

            /// Create a new ID using UUID v4 (random).
            pub fn new_uuid() -> Self {
                Self(uuid::Uuid::new_v4().to_string())
            }

            /// View as string slice.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new_uuid()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl From<String> for $name {
            fn from(s: String) -> Self {
                Self(s)
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                Self(s.to_string())
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }
    };
}

typed_id!(
    /// Unique identifier for an event.
    EventId
);
typed_id!(
    /// Unique identifier for a session.
    SessionId
);
typed_id!(
    /// Identifier for a branch within a session. Default is "main".
    BranchId
);
typed_id!(
    /// Unique identifier for an agent run (LLM invocation cycle).
    RunId
);
typed_id!(
    /// Unique identifier for a snapshot/checkpoint.
    SnapshotId
);
typed_id!(
    /// Unique identifier for an approval request.
    ApprovalId
);
typed_id!(
    /// Unique identifier for a memory entry.
    MemoryId
);
typed_id!(
    /// Unique identifier for a tool execution run.
    ToolRunId
);
typed_id!(
    /// Unique identifier for a checkpoint.
    CheckpointId
);

impl BranchId {
    /// The default "main" branch.
    pub fn main() -> Self {
        Self("main".to_owned())
    }
}

/// Content-addressed blob hash (SHA-256 hex string).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BlobHash(String);

impl BlobHash {
    pub fn from_hex(hex: impl Into<String>) -> Self {
        Self(hex.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for BlobHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for BlobHash {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl AsRef<str> for BlobHash {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Monotonic sequence number within a branch.
pub type SeqNo = u64;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_id_new_is_unique() {
        let a = EventId::new_uuid();
        let b = EventId::new_uuid();
        assert_ne!(a, b);
    }

    #[test]
    fn session_id_from_string() {
        let id = SessionId::from_string("test-session");
        assert_eq!(id.as_str(), "test-session");
        assert_eq!(id.to_string(), "test-session");
    }

    #[test]
    fn branch_id_main() {
        let id = BranchId::main();
        assert_eq!(id.as_str(), "main");
    }

    #[test]
    fn branch_id_from_str_trait() {
        let id: BranchId = "dev".into();
        assert_eq!(id.as_str(), "dev");
    }

    #[test]
    fn typed_id_serde_roundtrip() {
        let id = EventId::from_string("EVT001");
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"EVT001\"");
        let back: EventId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
    }

    #[test]
    fn typed_id_hash_equality() {
        use std::collections::HashSet;
        let a = SessionId::from_string("same");
        let b = SessionId::from_string("same");
        let mut set = HashSet::new();
        set.insert(a);
        assert!(set.contains(&b));
    }

    #[test]
    fn blob_hash_serde_roundtrip() {
        let hash = BlobHash::from_hex("deadbeef");
        let json = serde_json::to_string(&hash).unwrap();
        assert_eq!(json, "\"deadbeef\"");
        let back: BlobHash = serde_json::from_str(&json).unwrap();
        assert_eq!(hash, back);
    }

    #[test]
    fn memory_id_uniqueness() {
        let a = MemoryId::new_uuid();
        let b = MemoryId::new_uuid();
        assert_ne!(a, b);
    }
}
