//! Content-addressed blob storage DTOs (served by lagod).
//!
//! `BlobHash` is a re-export of [`crate::ids::BlobHash`] — both modules
//! previously carried a distinct struct. Phase 1 of Spec B.1 unified
//! them so the `BlobStorePort` wire type and the `ids::BlobHash` used
//! across the event record are the same symbol. Earlier code that
//! imported `aios_protocol::blob::BlobHash` still compiles unchanged.

use serde::{Deserialize, Serialize};

pub use crate::ids::BlobHash;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct BlobMetadata {
    pub hash: BlobHash,
    pub size_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
