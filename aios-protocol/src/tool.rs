//! Tool types: calls, outcomes, definitions, results, and the canonical Tool trait.
//!
//! This module provides the shared vocabulary for tool execution across all
//! Agent OS projects. Tool implementations (in Praxis or other runtimes)
//! implement the [`Tool`] trait defined here.

use crate::policy::Capability;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;

// ── Existing types (stable) ───────────────────────────────────────────

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

/// Tool execution outcome (kernel-level, simplified).
///
/// Used at the kernel boundary ([`ToolExecutionReport`](crate::ports::ToolExecutionReport)).
/// For richer tool results with typed content, see [`ToolResult`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ToolOutcome {
    Success { output: serde_json::Value },
    Failure { error: String },
}

// ── MCP-compatible behavioral annotations ─────────────────────────────

/// Behavioral annotations for tools (MCP-compatible).
///
/// These hints inform the runtime about a tool's side effects,
/// enabling policy enforcement and user confirmation flows.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ToolAnnotations {
    /// Tool does not modify its environment.
    #[serde(default)]
    pub read_only: bool,
    /// Tool may perform destructive updates.
    #[serde(default)]
    pub destructive: bool,
    /// Repeated calls with same args produce same result.
    #[serde(default)]
    pub idempotent: bool,
    /// Tool interacts with external entities (network, APIs).
    #[serde(default)]
    pub open_world: bool,
    /// Tool requires user confirmation before execution.
    #[serde(default)]
    pub requires_confirmation: bool,
}

// ── Tool definition ───────────────────────────────────────────────────

/// Complete description of a tool's interface and behavior.
///
/// This is the canonical tool definition used across all Agent OS projects.
/// It is MCP-aligned with additional fields for categorization and timeouts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolDefinition {
    /// Unique tool name (e.g. "read_file", "bash").
    pub name: String,
    /// Human-readable description of what the tool does.
    pub description: String,
    /// JSON Schema describing the tool's input parameters.
    pub input_schema: serde_json::Value,

    // ── MCP-aligned fields (all optional, backward-compatible) ──
    /// Human-readable display name (MCP: title).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// JSON Schema for structured output (MCP: outputSchema).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<serde_json::Value>,
    /// Behavioral hints (MCP: annotations).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<ToolAnnotations>,

    // ── Agent OS extensions ──
    /// Tool category for grouping ("filesystem", "code", "shell", "mcp").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// Tags for filtering and matching.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Maximum execution timeout in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u32>,
}

// ── Client-supplied tool definitions ──────────────────────────────────

/// A tool declared by the chat *client* rather than the kernel's own
/// governed registry.
///
/// These arrive over the substrate wire (`DispatchMessageReq.tool_definitions`,
/// one JSON object per entry in the AI-SDK / OpenAI function shape
/// `{"name", "description", "parameters"}`). They live in a different
/// trust domain from registry tools: the kernel never executes them —
/// it surfaces them to the model and, when the model proposes one,
/// hands the call back to the client as a `TOOL_CALL_PENDING` frame
/// (category `"client"`). The client executes the tool and replays the
/// result as conversation context on the next dispatch.
///
/// Because they are never policy-gated or harness-executed, they carry
/// no [`Capability`] set — only the model-visible surface (name,
/// description, JSON-Schema parameters passed through verbatim).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClientToolDefinition {
    /// Unique tool name as the model will reference it.
    pub name: String,
    /// Human-readable description of what the tool does.
    #[serde(default)]
    pub description: String,
    /// JSON Schema describing the tool's input parameters. Passed
    /// through to the provider verbatim (OpenAI `function.parameters`).
    #[serde(default)]
    pub parameters: serde_json::Value,
}

impl ClientToolDefinition {
    /// Parse one wire entry (a single JSON object in the AI-SDK /
    /// OpenAI function shape `{"name", "description", "parameters"}`).
    ///
    /// Returns an error when the bytes are not valid JSON, are not a
    /// JSON object, carry an empty/missing `name`, carry a non-string
    /// `description`, or carry a non-object `parameters`. Absent
    /// `description`/`parameters` stay permissive (empty string / Null)
    /// — absence is well-formed, a wrong TYPE is malformed. Callers at
    /// the trust boundary (the substrate dispatch handler) treat these
    /// errors as "skip this entry" rather than failing the whole
    /// dispatch.
    pub fn from_wire_bytes(bytes: &[u8]) -> Result<Self, ClientToolDefinitionError> {
        let value: serde_json::Value =
            serde_json::from_slice(bytes).map_err(ClientToolDefinitionError::Json)?;
        Self::from_value(value)
    }

    /// Parse one already-deserialized JSON value. Shares the validation
    /// rules with [`Self::from_wire_bytes`].
    pub fn from_value(value: serde_json::Value) -> Result<Self, ClientToolDefinitionError> {
        let obj = value
            .as_object()
            .ok_or(ClientToolDefinitionError::NotAnObject)?;
        let name = obj
            .get("name")
            .and_then(|v| v.as_str())
            .map(str::to_owned)
            .filter(|n| !n.is_empty())
            .ok_or(ClientToolDefinitionError::MissingName)?;
        let description = match obj.get("description") {
            Some(v) => v
                .as_str()
                .map(str::to_owned)
                .ok_or(ClientToolDefinitionError::InvalidDescriptionType)?,
            None => String::new(),
        };
        let parameters = match obj.get("parameters") {
            Some(v) if v.is_object() => v.clone(),
            Some(_) => return Err(ClientToolDefinitionError::InvalidParametersType),
            None => serde_json::Value::Null,
        };
        Ok(Self {
            name,
            description,
            parameters,
        })
    }
}

/// Reasons a client tool-definition wire entry was rejected.
///
/// `thiserror` per the library convention; callers downgrade these to
/// `warn!`-and-skip at the dispatch boundary so a malformed entry never
/// aborts an otherwise-valid turn.
#[derive(Debug, thiserror::Error)]
pub enum ClientToolDefinitionError {
    #[error("client tool definition is not valid JSON: {0}")]
    Json(serde_json::Error),
    #[error("client tool definition is not a JSON object")]
    NotAnObject,
    #[error("client tool definition is missing a non-empty `name`")]
    MissingName,
    #[error("client tool definition has a non-string `description`")]
    InvalidDescriptionType,
    #[error("client tool definition has a non-object `parameters`")]
    InvalidParametersType,
}

// ── Typed content blocks ──────────────────────────────────────────────

/// Typed content block in a tool result (MCP-compatible).
///
/// Tools can return structured content alongside the legacy JSON `output` field.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolContent {
    Text { text: String },
    Image { data: String, mime_type: String },
    Json { value: serde_json::Value },
}

// ── Rich tool result ──────────────────────────────────────────────────

/// Rich tool execution result with typed content.
///
/// This is the canonical result type returned by [`Tool::execute`].
/// It includes both a legacy JSON `output` and optional MCP-style
/// typed content blocks for richer responses.
///
/// For the simplified kernel-level outcome, see [`ToolOutcome`].
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ToolResult {
    pub call_id: String,
    pub tool_name: String,
    /// Legacy JSON output (always present for backward compatibility).
    #[serde(default)]
    pub output: serde_json::Value,
    /// MCP-style typed content blocks (optional, alongside output).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<Vec<ToolContent>>,
    /// Whether this result represents an error (MCP: isError).
    #[serde(default)]
    pub is_error: bool,
    /// Optional kernel-tier resource usage reported by the backend.
    ///
    /// Populated by kernel dispatch paths (see [`crate::budget::ResourceUsage`]).
    /// Legacy tool runtimes leave this `None`; additive and backward-compatible.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<crate::budget::ResourceUsage>,
}

impl ToolResult {
    /// Create a successful text result.
    pub fn text(call_id: impl Into<String>, tool_name: impl Into<String>, text: &str) -> Self {
        Self {
            call_id: call_id.into(),
            tool_name: tool_name.into(),
            output: serde_json::Value::String(text.to_string()),
            content: Some(vec![ToolContent::Text {
                text: text.to_string(),
            }]),
            is_error: false,
            usage: None,
        }
    }

    /// Create a successful JSON result.
    pub fn json(
        call_id: impl Into<String>,
        tool_name: impl Into<String>,
        value: serde_json::Value,
    ) -> Self {
        Self {
            call_id: call_id.into(),
            tool_name: tool_name.into(),
            output: value.clone(),
            content: Some(vec![ToolContent::Json { value }]),
            is_error: false,
            usage: None,
        }
    }

    /// Create an error result.
    pub fn error(call_id: impl Into<String>, tool_name: impl Into<String>, message: &str) -> Self {
        Self {
            call_id: call_id.into(),
            tool_name: tool_name.into(),
            output: serde_json::json!({ "error": message }),
            content: Some(vec![ToolContent::Text {
                text: message.to_string(),
            }]),
            is_error: true,
            usage: None,
        }
    }
}

/// Convert a rich `ToolResult` to a simplified `ToolOutcome` for kernel boundaries.
impl From<&ToolResult> for ToolOutcome {
    fn from(result: &ToolResult) -> Self {
        if result.is_error {
            ToolOutcome::Failure {
                error: match &result.output {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                },
            }
        } else {
            ToolOutcome::Success {
                output: result.output.clone(),
            }
        }
    }
}

// ── Tool execution context ────────────────────────────────────────────

/// Context provided to a tool during execution.
///
/// Contains the identifiers for the current run, session, and iteration
/// so tools can correlate their actions with the agent loop. Kernel-tier
/// execution paths additionally populate [`Self::wallet`], [`Self::cost_hint`],
/// and [`Self::kernel_ctx`] so tool implementations can propagate attribution
/// and budget signals to downstream metering / gating.
///
/// All kernel-tier fields are optional and additive: legacy tools that do not
/// care about attribution continue to work unchanged, and the serialized form
/// stays compatible with consumers deserializing the pre-kernel shape.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolContext {
    pub run_id: String,
    pub session_id: String,
    pub iteration: u32,
    /// Wallet attribution for on-chain settlement of this tool call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wallet: Option<crate::kernel::WalletAttribution>,
    /// Advisory cost hint consulted by the kernel budget gate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_hint: Option<crate::budget::ResourceBudget>,
    /// Kernel-tier dispatch context (session, agent, trace, …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kernel_ctx: Option<crate::kernel::KernelContext>,
}

// ── Tool errors ───────────────────────────────────────────────────────

/// Errors that can occur during tool execution.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("tool not found: {tool_name}")]
    NotFound { tool_name: String },

    #[error("[{tool_name}] execution failed: {message}")]
    ExecutionFailed { tool_name: String, message: String },

    #[error("invalid input: {message}")]
    InvalidInput { message: String },

    #[error("[{tool_name}] timed out after {timeout_secs}s")]
    Timeout {
        tool_name: String,
        timeout_secs: u32,
    },

    #[error("workspace policy violation: {message}")]
    PolicyViolation { message: String },

    #[error("{0}")]
    Other(String),
}

// ── Canonical Tool trait ──────────────────────────────────────────────

/// The canonical tool interface for the Agent OS.
///
/// All tool implementations (filesystem, shell, MCP bridges, skills)
/// implement this trait. The trait is synchronous — runtimes wrap
/// execution in `spawn_blocking` when needed.
///
/// # Object Safety
///
/// This trait is dyn-compatible (`Arc<dyn Tool>`) for use in registries.
pub trait Tool: Send + Sync {
    /// Returns the tool's definition (name, schema, annotations).
    fn definition(&self) -> ToolDefinition;

    /// Execute the tool with the given call and context.
    fn execute(&self, call: &ToolCall, ctx: &ToolContext) -> Result<ToolResult, ToolError>;
}

// ── Tool registry ─────────────────────────────────────────────────────

/// A registry of named tools, used by the orchestrator to dispatch tool calls.
#[derive(Clone, Default)]
pub struct ToolRegistry {
    tools: BTreeMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    /// Register a tool. Replaces any existing tool with the same name.
    pub fn register<T: Tool + 'static>(&mut self, tool: T) {
        self.tools
            .insert(tool.definition().name.clone(), Arc::new(tool));
    }

    /// Register a pre-wrapped `Arc<dyn Tool>`.
    pub fn register_arc(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.definition().name.clone(), tool);
    }

    /// Look up a tool by name.
    pub fn get(&self, tool_name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(tool_name).cloned()
    }

    /// Return definitions for all registered tools.
    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.values().map(|tool| tool.definition()).collect()
    }

    /// Return the number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Returns `true` if no tools are registered.
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Return all registered tool names.
    pub fn names(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }
}

impl std::fmt::Debug for ToolRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolRegistry")
            .field("tools", &self.tools.keys().collect::<Vec<_>>())
            .finish()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── Existing tests ──

    #[test]
    fn tool_call_new() {
        let tc = ToolCall::new("read_file", json!({"path": "/tmp"}), vec![]);
        assert_eq!(tc.tool_name, "read_file");
        assert!(!tc.call_id.is_empty());
    }

    #[test]
    fn tool_outcome_serde_roundtrip() {
        let success = ToolOutcome::Success {
            output: json!({"data": 42}),
        };
        let json_str = serde_json::to_string(&success).unwrap();
        assert!(json_str.contains("\"status\":\"success\""));
        let back: ToolOutcome = serde_json::from_str(&json_str).unwrap();
        assert!(matches!(back, ToolOutcome::Success { .. }));

        let failure = ToolOutcome::Failure {
            error: "not found".into(),
        };
        let json_str = serde_json::to_string(&failure).unwrap();
        assert!(json_str.contains("\"status\":\"failure\""));
    }

    // ── ToolAnnotations tests ──

    #[test]
    fn annotations_default_all_false() {
        let ann = ToolAnnotations::default();
        assert!(!ann.read_only);
        assert!(!ann.destructive);
        assert!(!ann.idempotent);
        assert!(!ann.open_world);
        assert!(!ann.requires_confirmation);
    }

    #[test]
    fn annotations_serde_roundtrip() {
        let ann = ToolAnnotations {
            read_only: true,
            destructive: false,
            idempotent: true,
            open_world: false,
            requires_confirmation: true,
        };
        let json_str = serde_json::to_string(&ann).unwrap();
        let back: ToolAnnotations = serde_json::from_str(&json_str).unwrap();
        assert_eq!(ann, back);
    }

    #[test]
    fn annotations_missing_fields_default_false() {
        let json_str = r#"{"read_only": true}"#;
        let ann: ToolAnnotations = serde_json::from_str(json_str).unwrap();
        assert!(ann.read_only);
        assert!(!ann.destructive);
    }

    // ── ToolDefinition tests ──

    #[test]
    fn tool_definition_minimal() {
        let def = ToolDefinition {
            name: "test_tool".into(),
            description: "A test tool".into(),
            input_schema: json!({"type": "object"}),
            title: None,
            output_schema: None,
            annotations: None,
            category: None,
            tags: vec![],
            timeout_secs: None,
        };
        let json_str = serde_json::to_string(&def).unwrap();
        // Optional fields should be omitted
        assert!(!json_str.contains("title"));
        assert!(!json_str.contains("tags"));
        let back: ToolDefinition = serde_json::from_str(&json_str).unwrap();
        assert_eq!(def, back);
    }

    #[test]
    fn tool_definition_full() {
        let def = ToolDefinition {
            name: "read_file".into(),
            description: "Read a file from the workspace".into(),
            input_schema: json!({
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            }),
            title: Some("Read File".into()),
            output_schema: Some(json!({"type": "string"})),
            annotations: Some(ToolAnnotations {
                read_only: true,
                idempotent: true,
                ..Default::default()
            }),
            category: Some("filesystem".into()),
            tags: vec!["fs".into(), "read".into()],
            timeout_secs: Some(30),
        };
        let json_str = serde_json::to_string(&def).unwrap();
        let back: ToolDefinition = serde_json::from_str(&json_str).unwrap();
        assert_eq!(def, back);
        assert!(json_str.contains("\"category\":\"filesystem\""));
    }

    // ── ClientToolDefinition tests ──

    #[test]
    fn client_tool_def_from_wire_bytes_full_shape() {
        let bytes = br#"{
            "name": "get_weather",
            "description": "Look up the weather",
            "parameters": {
                "type": "object",
                "properties": { "city": { "type": "string" } },
                "required": ["city"]
            }
        }"#;
        let def = ClientToolDefinition::from_wire_bytes(bytes).expect("valid client tool");
        assert_eq!(def.name, "get_weather");
        assert_eq!(def.description, "Look up the weather");
        // parameters passed through verbatim.
        assert_eq!(def.parameters["properties"]["city"]["type"], "string");
        assert_eq!(def.parameters["required"][0], "city");
    }

    #[test]
    fn client_tool_def_missing_description_and_parameters_defaults() {
        let bytes = br#"{ "name": "ping" }"#;
        let def = ClientToolDefinition::from_wire_bytes(bytes).expect("name-only is valid");
        assert_eq!(def.name, "ping");
        assert_eq!(def.description, "");
        assert_eq!(def.parameters, serde_json::Value::Null);
    }

    #[test]
    fn client_tool_def_rejects_invalid_json() {
        let err = ClientToolDefinition::from_wire_bytes(b"{not json").unwrap_err();
        assert!(matches!(err, ClientToolDefinitionError::Json(_)));
    }

    #[test]
    fn client_tool_def_rejects_non_object() {
        let err = ClientToolDefinition::from_wire_bytes(b"[1, 2, 3]").unwrap_err();
        assert!(matches!(err, ClientToolDefinitionError::NotAnObject));
    }

    #[test]
    fn client_tool_def_rejects_wrong_field_types() {
        // Present-but-mistyped fields are malformed (warn+skip at the
        // boundary); absence stays permissive — covered by
        // client_tool_def_missing_description_and_parameters_defaults.
        let desc = ClientToolDefinition::from_wire_bytes(br#"{"name": "t", "description": 7}"#)
            .unwrap_err();
        assert!(matches!(
            desc,
            ClientToolDefinitionError::InvalidDescriptionType
        ));
        let params =
            ClientToolDefinition::from_wire_bytes(br#"{"name": "t", "parameters": "str"}"#)
                .unwrap_err();
        assert!(matches!(
            params,
            ClientToolDefinitionError::InvalidParametersType
        ));
    }

    #[test]
    fn client_tool_def_rejects_missing_or_empty_name() {
        let missing =
            ClientToolDefinition::from_wire_bytes(br#"{"description": "x"}"#).unwrap_err();
        assert!(matches!(missing, ClientToolDefinitionError::MissingName));
        let empty = ClientToolDefinition::from_wire_bytes(br#"{"name": ""}"#).unwrap_err();
        assert!(matches!(empty, ClientToolDefinitionError::MissingName));
    }

    #[test]
    fn client_tool_def_serde_roundtrip() {
        let def = ClientToolDefinition {
            name: "search".into(),
            description: "Search the web".into(),
            parameters: json!({"type": "object"}),
        };
        let json_str = serde_json::to_string(&def).unwrap();
        let back: ClientToolDefinition = serde_json::from_str(&json_str).unwrap();
        assert_eq!(def, back);
    }

    // ── ToolContent tests ──

    #[test]
    fn tool_content_text_serde() {
        let content = ToolContent::Text {
            text: "hello".into(),
        };
        let json_str = serde_json::to_string(&content).unwrap();
        assert!(json_str.contains("\"type\":\"text\""));
        let back: ToolContent = serde_json::from_str(&json_str).unwrap();
        assert_eq!(content, back);
    }

    #[test]
    fn tool_content_json_serde() {
        let content = ToolContent::Json {
            value: json!({"key": "value"}),
        };
        let json_str = serde_json::to_string(&content).unwrap();
        assert!(json_str.contains("\"type\":\"json\""));
        let back: ToolContent = serde_json::from_str(&json_str).unwrap();
        assert_eq!(content, back);
    }

    #[test]
    fn tool_content_image_serde() {
        let content = ToolContent::Image {
            data: "base64data".into(),
            mime_type: "image/png".into(),
        };
        let json_str = serde_json::to_string(&content).unwrap();
        let back: ToolContent = serde_json::from_str(&json_str).unwrap();
        assert_eq!(content, back);
    }

    // ── ToolResult tests ──

    #[test]
    fn tool_result_text_helper() {
        let result = ToolResult::text("call-1", "echo", "hello world");
        assert_eq!(result.call_id, "call-1");
        assert_eq!(result.tool_name, "echo");
        assert!(!result.is_error);
        assert!(result.content.is_some());
    }

    #[test]
    fn tool_result_json_helper() {
        let result = ToolResult::json("call-2", "search", json!({"matches": 5}));
        assert!(!result.is_error);
        assert_eq!(result.output, json!({"matches": 5}));
    }

    #[test]
    fn tool_result_error_helper() {
        let result = ToolResult::error("call-3", "bash", "permission denied");
        assert!(result.is_error);
        assert_eq!(result.output["error"], "permission denied");
    }

    #[test]
    fn tool_result_serde_roundtrip() {
        let result = ToolResult {
            call_id: "c1".into(),
            tool_name: "test".into(),
            output: json!({"ok": true}),
            content: Some(vec![ToolContent::Text {
                text: "success".into(),
            }]),
            is_error: false,
            usage: None,
        };
        let json_str = serde_json::to_string(&result).unwrap();
        let back: ToolResult = serde_json::from_str(&json_str).unwrap();
        assert_eq!(result, back);
    }

    // ── ToolResult → ToolOutcome conversion ──

    #[test]
    fn tool_result_to_outcome_success() {
        let result = ToolResult::json("c1", "test", json!({"data": 42}));
        let outcome: ToolOutcome = ToolOutcome::from(&result);
        assert!(matches!(outcome, ToolOutcome::Success { .. }));
    }

    #[test]
    fn tool_result_to_outcome_failure() {
        let result = ToolResult::error("c1", "test", "oops");
        let outcome: ToolOutcome = ToolOutcome::from(&result);
        match outcome {
            ToolOutcome::Failure { error } => assert!(error.contains("oops")),
            _ => panic!("expected failure"),
        }
    }

    // ── Tool trait + Registry tests ──

    struct EchoTool;

    impl Tool for EchoTool {
        fn definition(&self) -> ToolDefinition {
            ToolDefinition {
                name: "echo".into(),
                description: "Echoes the input value".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": { "value": { "type": "string" } },
                    "required": ["value"]
                }),
                title: None,
                output_schema: None,
                annotations: Some(ToolAnnotations {
                    read_only: true,
                    idempotent: true,
                    ..Default::default()
                }),
                category: Some("test".into()),
                tags: vec![],
                timeout_secs: Some(10),
            }
        }

        fn execute(&self, call: &ToolCall, _ctx: &ToolContext) -> Result<ToolResult, ToolError> {
            let value = call.input.get("value").cloned().unwrap_or(json!(null));
            Ok(ToolResult::json(&call.call_id, &call.tool_name, value))
        }
    }

    struct FailTool;

    impl Tool for FailTool {
        fn definition(&self) -> ToolDefinition {
            ToolDefinition {
                name: "fail".into(),
                description: "Always fails".into(),
                input_schema: json!({"type": "object"}),
                title: None,
                output_schema: None,
                annotations: None,
                category: None,
                tags: vec![],
                timeout_secs: None,
            }
        }

        fn execute(&self, call: &ToolCall, _ctx: &ToolContext) -> Result<ToolResult, ToolError> {
            Err(ToolError::ExecutionFailed {
                tool_name: call.tool_name.clone(),
                message: "always fails".into(),
            })
        }
    }

    fn test_context() -> ToolContext {
        ToolContext {
            run_id: "run-1".into(),
            session_id: "sess-1".into(),
            iteration: 1,
            ..Default::default()
        }
    }

    #[test]
    fn tool_trait_execute_success() {
        let tool = EchoTool;
        let call = ToolCall::new("echo", json!({"value": "hello"}), vec![]);
        let result = tool.execute(&call, &test_context()).unwrap();
        assert!(!result.is_error);
        assert_eq!(result.output, json!("hello"));
    }

    #[test]
    fn tool_trait_execute_error() {
        let tool = FailTool;
        let call = ToolCall::new("fail", json!({}), vec![]);
        let err = tool.execute(&call, &test_context()).unwrap_err();
        assert!(matches!(err, ToolError::ExecutionFailed { .. }));
        assert!(err.to_string().contains("always fails"));
    }

    #[test]
    fn registry_register_and_get() {
        let mut reg = ToolRegistry::default();
        assert!(reg.is_empty());

        reg.register(EchoTool);
        assert_eq!(reg.len(), 1);
        assert!(!reg.is_empty());

        let tool = reg.get("echo").expect("should find echo");
        let def = tool.definition();
        assert_eq!(def.name, "echo");
    }

    #[test]
    fn registry_get_missing() {
        let reg = ToolRegistry::default();
        assert!(reg.get("nonexistent").is_none());
    }

    #[test]
    fn registry_definitions() {
        let mut reg = ToolRegistry::default();
        reg.register(EchoTool);
        reg.register(FailTool);

        let defs = reg.definitions();
        assert_eq!(defs.len(), 2);
        let names: Vec<_> = defs.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"echo"));
        assert!(names.contains(&"fail"));
    }

    #[test]
    fn registry_names() {
        let mut reg = ToolRegistry::default();
        reg.register(EchoTool);
        reg.register(FailTool);

        let names = reg.names();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"echo".to_string()));
        assert!(names.contains(&"fail".to_string()));
    }

    #[test]
    fn registry_register_replaces_existing() {
        let mut reg = ToolRegistry::default();
        reg.register(EchoTool);
        reg.register(EchoTool); // same name, should replace
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn registry_debug_format() {
        let mut reg = ToolRegistry::default();
        reg.register(EchoTool);
        let debug = format!("{:?}", reg);
        assert!(debug.contains("echo"));
    }

    // ── ToolError tests ──

    #[test]
    fn tool_error_display() {
        let err = ToolError::NotFound {
            tool_name: "ghost".into(),
        };
        assert_eq!(err.to_string(), "tool not found: ghost");

        let err = ToolError::Timeout {
            tool_name: "slow".into(),
            timeout_secs: 30,
        };
        assert_eq!(err.to_string(), "[slow] timed out after 30s");
    }
}

// ── Kernel-tier additive-field tests ──────────────────────────────────
//
// These live in their own submodule so the ToolContext / ToolResult
// legacy behavior is easy to locate and the kernel-tier coverage is
// explicit.
#[cfg(test)]
mod kernel_ext_tests {
    use super::*;

    #[test]
    fn tool_context_default_has_none_kernel_fields() {
        let ctx = ToolContext::default();
        assert!(ctx.wallet.is_none());
        assert!(ctx.cost_hint.is_none());
        assert!(ctx.kernel_ctx.is_none());
        assert_eq!(ctx.iteration, 0);
        assert!(ctx.run_id.is_empty());
        assert!(ctx.session_id.is_empty());
    }

    #[test]
    fn tool_context_serializes_without_kernel_fields_when_none() {
        let ctx = ToolContext {
            run_id: "r1".into(),
            session_id: "s1".into(),
            iteration: 3,
            ..Default::default()
        };
        let json = serde_json::to_string(&ctx).unwrap();
        assert!(json.contains("\"run_id\":\"r1\""));
        assert!(json.contains("\"session_id\":\"s1\""));
        assert!(json.contains("\"iteration\":3"));
        // skip_serializing_if should strip the optional kernel fields.
        assert!(!json.contains("wallet"));
        assert!(!json.contains("cost_hint"));
        assert!(!json.contains("kernel_ctx"));
    }

    #[test]
    fn tool_context_deserializes_legacy_shape() {
        // Legacy JSON produced before the kernel-tier fields existed must
        // still deserialize cleanly — the promise of additive extensions.
        let legacy = r#"{"run_id":"r1","session_id":"s1","iteration":3}"#;
        let ctx: ToolContext = serde_json::from_str(legacy).unwrap();
        assert_eq!(ctx.run_id, "r1");
        assert_eq!(ctx.session_id, "s1");
        assert_eq!(ctx.iteration, 3);
        assert!(ctx.wallet.is_none());
        assert!(ctx.cost_hint.is_none());
        assert!(ctx.kernel_ctx.is_none());
    }

    #[test]
    fn tool_context_roundtrip_with_kernel_fields() {
        use crate::budget::ResourceBudget;
        use crate::kernel::{ChainId, WalletAttribution};

        let ctx = ToolContext {
            run_id: "r1".into(),
            session_id: "s1".into(),
            iteration: 7,
            wallet: Some(WalletAttribution {
                address: "0xabcdef".into(),
                chain: ChainId::base(),
            }),
            cost_hint: Some(ResourceBudget {
                max_cpu_ms: Some(500),
                ..Default::default()
            }),
            kernel_ctx: None,
        };
        let json = serde_json::to_string(&ctx).unwrap();
        assert!(json.contains("\"wallet\""));
        assert!(json.contains("\"cost_hint\""));

        let back: ToolContext = serde_json::from_str(&json).unwrap();
        assert_eq!(back.run_id, "r1");
        assert_eq!(back.iteration, 7);
        let wallet = back.wallet.expect("wallet roundtrips");
        assert_eq!(wallet.address, "0xabcdef");
        assert_eq!(wallet.chain, ChainId::base());
        let cost_hint = back.cost_hint.expect("cost_hint roundtrips");
        assert_eq!(cost_hint.max_cpu_ms, Some(500));
    }

    #[test]
    fn tool_result_default_usage_is_none() {
        let r = ToolResult::default();
        assert!(r.usage.is_none());
        assert_eq!(r.call_id, "");
        assert_eq!(r.tool_name, "");
        assert!(r.content.is_none());
        assert!(!r.is_error);
    }

    #[test]
    fn tool_result_helpers_leave_usage_none() {
        let text = ToolResult::text("c1", "t", "hello");
        assert!(text.usage.is_none());
        let json_r = ToolResult::json("c2", "t", serde_json::json!({"ok": true}));
        assert!(json_r.usage.is_none());
        let err = ToolResult::error("c3", "t", "boom");
        assert!(err.usage.is_none());
    }

    #[test]
    fn tool_result_deserializes_legacy_shape() {
        let legacy = r#"{"call_id":"c1","tool_name":"t","output":{"foo":1},"is_error":false}"#;
        let r: ToolResult = serde_json::from_str(legacy).unwrap();
        assert_eq!(r.call_id, "c1");
        assert_eq!(r.tool_name, "t");
        assert!(r.usage.is_none());
    }

    #[test]
    fn tool_result_serializes_without_usage_when_none() {
        let r = ToolResult::text("c1", "t", "hello");
        let json = serde_json::to_string(&r).unwrap();
        assert!(!json.contains("usage"));
    }

    #[test]
    fn tool_result_roundtrip_with_usage() {
        use crate::budget::{ResourceUsage, UsageConfidence};
        let r = ToolResult {
            usage: Some(ResourceUsage {
                cpu_ms: 100,
                mem_peak_kb: 2048,
                egress_bytes: 512,
                duration_ms: 120,
                syscall_count: 42,
                confidence: UsageConfidence::Measured,
            }),
            ..ToolResult::text("c1", "t", "hello")
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"usage\""));
        let back: ToolResult = serde_json::from_str(&json).unwrap();
        let usage = back.usage.as_ref().expect("usage roundtrips");
        assert_eq!(usage.cpu_ms, 100);
        assert_eq!(usage.mem_peak_kb, 2048);
        assert_eq!(usage.egress_bytes, 512);
        assert_eq!(usage.duration_ms, 120);
        assert_eq!(usage.syscall_count, 42);
        assert_eq!(usage.confidence, UsageConfidence::Measured);
        assert_eq!(back, r);
    }
}
