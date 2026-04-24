//! Content-addressed blob storage DTOs (served by lagod).

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct BlobHash(pub String);

impl BlobHash {
    pub fn from_sha256_hex(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct BlobMetadata {
    pub hash: BlobHash,
    pub size_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
