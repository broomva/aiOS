//! Tool types: calls, outcomes, and definitions.

use crate::policy::Capability;
use serde::{Deserialize, Serialize};

/// A tool invocation request with capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub call_id: String,
    pub tool_name: String,
    pub input: serde_json::Value,
    #[serde(default)]
    pub requested_capabilities: Vec<Capability>,
}

impl ToolCall {
    pub fn new(
        tool_name: impl Into<String>,
        input: serde_json::Value,
        requested_capabilities: Vec<Capability>,
    ) -> Self {
        Self {
            call_id: uuid::Uuid::new_v4().to_string(),
            tool_name: tool_name.into(),
            input,
            requested_capabilities,
        }
    }
}

/// Tool execution outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ToolOutcome {
    Success { output: serde_json::Value },
    Failure { error: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_call_new() {
        let tc = ToolCall::new("read_file", serde_json::json!({"path": "/tmp"}), vec![]);
        assert_eq!(tc.tool_name, "read_file");
        assert!(!tc.call_id.is_empty());
    }

    #[test]
    fn tool_outcome_serde_roundtrip() {
        let success = ToolOutcome::Success {
            output: serde_json::json!({"data": 42}),
        };
        let json = serde_json::to_string(&success).unwrap();
        assert!(json.contains("\"status\":\"success\""));
        let back: ToolOutcome = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, ToolOutcome::Success { .. }));

        let failure = ToolOutcome::Failure {
            error: "not found".into(),
        };
        let json = serde_json::to_string(&failure).unwrap();
        assert!(json.contains("\"status\":\"failure\""));
    }
}
