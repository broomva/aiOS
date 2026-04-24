//! Knowledge index + wikilink graph DTOs (served by lagod via lago-knowledge).

use crate::ids::SessionId;
use serde::{Deserialize, Serialize};

pub type NoteId = String;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct KnowledgeQuery {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_score: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<SessionId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct KnowledgeSearchResult {
    pub total: u32,
    pub hits: Vec<NoteHit>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct NoteHit {
    pub note_id: NoteId,
    pub title: String,
    pub score: f32,
    pub excerpt: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Note {
    pub id: NoteId,
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub frontmatter: serde_json::Value,
    #[serde(default)]
    pub outgoing: Vec<NoteEdge>,
    #[serde(default)]
    pub incoming: Vec<NoteEdge>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct NoteDraft {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<NoteId>,
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub frontmatter: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct NoteEdge {
    pub target: NoteId,
    pub kind: NoteEdgeKind,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum NoteEdgeKind {
    Wikilink,
    Tag,
    Embed,
    Reference,
}
