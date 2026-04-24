//! Metacognitive evaluation DTOs (served by nousd).

use crate::ids::SessionId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct EvaluationRequest {
    pub session: SessionId,
    pub artifact: serde_json::Value,
    pub rubric: String,
    #[serde(default)]
    pub context: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct EvaluationReport {
    pub verdict: String,
    pub score: f32,
    pub reasoning: String,
    #[serde(default)]
    pub dimensions: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct HeuristicScoreRequest {
    pub heuristic: String,
    pub input: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HeuristicScore {
    pub heuristic: String,
    pub score: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct JudgementRequest {
    pub question: String,
    pub candidates: Vec<serde_json::Value>,
    #[serde(default)]
    pub context: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct JudgementVerdict {
    pub winning_index: usize,
    pub confidence: f32,
    pub reasoning: String,
}
