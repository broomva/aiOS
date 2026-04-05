use serde_json::{Value, json};

pub fn openapi_spec() -> Value {
    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "aiOS Control Plane API",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "Session-oriented agent OS control plane with event-native streaming.",
        },
        "paths": {
            "/healthz": {
                "get": {
                    "summary": "Health check",
                    "responses": {
                        "200": {
                            "description": "Service health",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "required": ["status", "service"],
                                        "properties": {
                                            "status": { "type": "string" },
                                            "service": { "type": "string" },
                                        },
                                    },
                                },
                            },
                        },
                    },
                },
            },
            "/sessions": {
                "post": {
                    "summary": "Create session",
                    "requestBody": {
                        "required": false,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/CreateSessionRequest" },
                            },
                        },
                    },
                    "responses": {
                        "200": {
                            "description": "Session manifest",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/SessionManifest" },
                                },
                            },
                        },
                    },
                },
            },
            "/sessions/{session_id}/ticks": {
                "post": {
                    "summary": "Execute one kernel tick",
                    "parameters": [
                        { "$ref": "#/components/parameters/SessionIdPath" },
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/TickRequest" },
                            },
                        },
                    },
                    "responses": {
                        "200": {
                            "description": "Tick result",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/TickResponse" },
                                },
                            },
                        },
                    },
                },
            },
            "/sessions/{session_id}/branches": {
                "post": {
                    "summary": "Create branch",
                    "parameters": [
                        { "$ref": "#/components/parameters/SessionIdPath" },
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/CreateBranchRequest" },
                            },
                        },
                    },
                    "responses": {
                        "200": {
                            "description": "Branch info",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/BranchInfo" },
                                },
                            },
                        },
                    },
                },
                "get": {
                    "summary": "List branches for session",
                    "parameters": [
                        { "$ref": "#/components/parameters/SessionIdPath" },
                    ],
                    "responses": {
                        "200": {
                            "description": "Branch list",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/BranchListResponse" },
                                },
                            },
                        },
                    },
                },
            },
            "/sessions/{session_id}/branches/{branch_id}/merge": {
                "post": {
                    "summary": "Merge source branch into target branch",
                    "parameters": [
                        { "$ref": "#/components/parameters/SessionIdPath" },
                        { "$ref": "#/components/parameters/BranchPath" },
                    ],
                    "requestBody": {
                        "required": false,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/MergeBranchRequest" },
                            },
                        },
                    },
                    "responses": {
                        "200": {
                            "description": "Merge result",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/BranchMergeResponse" },
                                },
                            },
                        },
                    },
                },
            },
            "/sessions/{session_id}/approvals/{approval_id}": {
                "post": {
                    "summary": "Resolve approval gate",
                    "parameters": [
                        { "$ref": "#/components/parameters/SessionIdPath" },
                        { "$ref": "#/components/parameters/ApprovalIdPath" },
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/ResolveApprovalRequest" },
                            },
                        },
                    },
                    "responses": {
                        "204": { "description": "Approval resolved" },
                    },
                },
            },
            "/sessions/{session_id}/events": {
                "get": {
                    "summary": "List persisted events",
                    "parameters": [
                        { "$ref": "#/components/parameters/SessionIdPath" },
                        {
                            "name": "branch",
                            "in": "query",
                            "required": false,
                            "schema": { "type": "string", "default": "main" },
                        },
                        {
                            "name": "from_sequence",
                            "in": "query",
                            "required": false,
                            "schema": { "type": "integer", "format": "int64", "minimum": 1 },
                        },
                        {
                            "name": "limit",
                            "in": "query",
                            "required": false,
                            "schema": { "type": "integer", "format": "int32", "minimum": 1, "maximum": 5000 },
                        },
                    ],
                    "responses": {
                        "200": {
                            "description": "Event page",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/EventListResponse" },
                                },
                            },
                        },
                    },
                },
            },
            "/sessions/{session_id}/events/stream": {
                "get": {
                    "summary": "Stream raw kernel events over SSE",
                    "parameters": [
                        { "$ref": "#/components/parameters/SessionIdPath" },
                        { "$ref": "#/components/parameters/BranchQuery" },
                        { "$ref": "#/components/parameters/CursorQuery" },
                        { "$ref": "#/components/parameters/ReplayLimitQuery" },
                    ],
                    "responses": {
                        "200": {
                            "description": "text/event-stream of EventRecord payloads",
                            "content": {
                                "text/event-stream": {
                                    "schema": { "type": "string" },
                                },
                            },
                        },
                    },
                },
            },
            "/sessions/{session_id}/events/stream/vercel-ai-sdk-v6": {
                "get": {
                    "summary": "Stream Vercel AI SDK v6 UIMessage protocol over SSE",
                    "parameters": [
                        { "$ref": "#/components/parameters/SessionIdPath" },
                        { "$ref": "#/components/parameters/BranchQuery" },
                        { "$ref": "#/components/parameters/CursorQuery" },
                        { "$ref": "#/components/parameters/ReplayLimitQuery" },
                    ],
                    "responses": {
                        "200": {
                            "description": "Vercel AI SDK v6 stream parts",
                            "headers": {
                                "x-vercel-ai-ui-message-stream": {
                                    "description": "UI stream protocol version",
                                    "schema": { "type": "string", "enum": ["v1"] },
                                },
                            },
                            "content": {
                                "text/event-stream": {
                                    "schema": { "type": "string" },
                                },
                            },
                        },
                    },
                },
            },
            "/sessions/{session_id}/voice/start": {
                "post": {
                    "summary": "Start voice adapter session",
                    "parameters": [
                        { "$ref": "#/components/parameters/SessionIdPath" },
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/VoiceStartRequest" },
                            },
                        },
                    },
                    "responses": {
                        "200": {
                            "description": "Voice session started",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/VoiceStartResponse" },
                                },
                            },
                        },
                    },
                },
            },
            "/sessions/{session_id}/voice/stream": {
                "get": {
                    "summary": "Bidirectional audio stream via WebSocket",
                    "parameters": [
                        { "$ref": "#/components/parameters/SessionIdPath" },
                        {
                            "name": "voice_session_id",
                            "in": "query",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" },
                        },
                    ],
                    "responses": {
                        "101": { "description": "WebSocket upgrade" },
                    },
                },
            },
            "/openapi.json": {
                "get": {
                    "summary": "OpenAPI specification document",
                    "responses": {
                        "200": {
                            "description": "OpenAPI JSON",
                            "content": {
                                "application/json": {
                                    "schema": { "type": "object" },
                                },
                            },
                        },
                    },
                },
            },
            "/docs": {
                "get": {
                    "summary": "Interactive Scalar API reference",
                    "responses": {
                        "200": {
                            "description": "HTML page",
                            "content": {
                                "text/html": {
                                    "schema": { "type": "string" },
                                },
                            },
                        },
                    },
                },
            },
        },
        "components": {
            "parameters": {
                "SessionIdPath": {
                    "name": "session_id",
                    "in": "path",
                    "required": true,
                    "schema": { "type": "string", "format": "uuid" },
                },
                "ApprovalIdPath": {
                    "name": "approval_id",
                    "in": "path",
                    "required": true,
                    "schema": { "type": "string", "format": "uuid" },
                },
                "BranchPath": {
                    "name": "branch_id",
                    "in": "path",
                    "required": true,
                    "schema": { "type": "string" },
                },
                "CursorQuery": {
                    "name": "cursor",
                    "in": "query",
                    "required": false,
                    "schema": { "type": "integer", "format": "int64", "minimum": 0 },
                },
                "BranchQuery": {
                    "name": "branch",
                    "in": "query",
                    "required": false,
                    "schema": { "type": "string", "default": "main" },
                },
                "ReplayLimitQuery": {
                    "name": "replay_limit",
                    "in": "query",
                    "required": false,
                    "schema": { "type": "integer", "format": "int32", "minimum": 1, "maximum": 5000 },
                },
            },
            "schemas": {
                "CreateSessionRequest": {
                    "type": "object",
                    "properties": {
                        "owner": { "type": "string" },
                        "policy": { "$ref": "#/components/schemas/PolicySet" },
                        "model_routing": { "$ref": "#/components/schemas/ModelRouting" },
                    },
                },
                "ModelRouting": {
                    "type": "object",
                    "required": ["primary_model", "fallback_models", "temperature"],
                    "properties": {
                        "primary_model": { "type": "string" },
                        "fallback_models": { "type": "array", "items": { "type": "string" } },
                        "temperature": { "type": "number" },
                    },
                },
                "PolicySet": {
                    "type": "object",
                    "required": ["allow_capabilities", "gate_capabilities", "max_tool_runtime_secs", "max_events_per_turn"],
                    "properties": {
                        "allow_capabilities": { "type": "array", "items": { "type": "string" } },
                        "gate_capabilities": { "type": "array", "items": { "type": "string" } },
                        "max_tool_runtime_secs": { "type": "integer", "format": "int64", "minimum": 0 },
                        "max_events_per_turn": { "type": "integer", "format": "int64", "minimum": 1 },
                    },
                },
                "SessionManifest": {
                    "type": "object",
                    "required": ["session_id", "owner", "created_at", "workspace_root", "model_routing", "policy"],
                    "properties": {
                        "session_id": { "type": "string", "format": "uuid" },
                        "owner": { "type": "string" },
                        "created_at": { "type": "string", "format": "date-time" },
                        "workspace_root": { "type": "string" },
                        "model_routing": { "$ref": "#/components/schemas/ModelRouting" },
                        "policy": { "$ref": "#/components/schemas/PolicySet" },
                    },
                },
                "ProposedToolRequest": {
                    "type": "object",
                    "required": ["tool_name", "input", "requested_capabilities"],
                    "properties": {
                        "tool_name": { "type": "string" },
                        "input": { "type": "object", "additionalProperties": true },
                        "requested_capabilities": { "type": "array", "items": { "type": "string" } },
                    },
                },
                "TickRequest": {
                    "type": "object",
                    "required": ["objective"],
                    "properties": {
                        "objective": { "type": "string" },
                        "branch": { "type": "string", "default": "main" },
                        "proposed_tool": { "$ref": "#/components/schemas/ProposedToolRequest" },
                    },
                },
                "CreateBranchRequest": {
                    "type": "object",
                    "required": ["branch"],
                    "properties": {
                        "branch": { "type": "string" },
                        "from_branch": { "type": "string", "default": "main" },
                        "fork_sequence": { "type": "integer", "format": "int64", "minimum": 0 },
                    },
                },
                "BranchInfo": {
                    "type": "object",
                    "required": ["branch_id", "fork_sequence", "head_sequence"],
                    "properties": {
                        "branch_id": { "type": "string" },
                        "parent_branch": { "type": "string", "nullable": true },
                        "fork_sequence": { "type": "integer", "format": "int64", "minimum": 0 },
                        "head_sequence": { "type": "integer", "format": "int64", "minimum": 0 },
                        "merged_into": { "type": "string", "nullable": true },
                    },
                },
                "BranchListResponse": {
                    "type": "object",
                    "required": ["session_id", "branches"],
                    "properties": {
                        "session_id": { "type": "string", "format": "uuid" },
                        "branches": { "type": "array", "items": { "$ref": "#/components/schemas/BranchInfo" } },
                    },
                },
                "MergeBranchRequest": {
                    "type": "object",
                    "properties": {
                        "target_branch": { "type": "string", "default": "main" },
                    },
                },
                "BranchMergeResult": {
                    "type": "object",
                    "required": ["source_branch", "target_branch", "source_head_sequence", "target_head_sequence"],
                    "properties": {
                        "source_branch": { "type": "string" },
                        "target_branch": { "type": "string" },
                        "source_head_sequence": { "type": "integer", "format": "int64", "minimum": 0 },
                        "target_head_sequence": { "type": "integer", "format": "int64", "minimum": 0 },
                    },
                },
                "BranchMergeResponse": {
                    "type": "object",
                    "required": ["session_id", "result"],
                    "properties": {
                        "session_id": { "type": "string", "format": "uuid" },
                        "result": { "$ref": "#/components/schemas/BranchMergeResult" },
                    },
                },
                "BudgetState": {
                    "type": "object",
                    "required": ["tokens_remaining", "time_remaining_ms", "cost_remaining_usd", "tool_calls_remaining", "error_budget_remaining"],
                    "properties": {
                        "tokens_remaining": { "type": "integer", "format": "int64", "minimum": 0 },
                        "time_remaining_ms": { "type": "integer", "format": "int64", "minimum": 0 },
                        "cost_remaining_usd": { "type": "number", "minimum": 0 },
                        "tool_calls_remaining": { "type": "integer", "format": "int32", "minimum": 0 },
                        "error_budget_remaining": { "type": "integer", "format": "int32", "minimum": 0 },
                    },
                },
                "AgentStateVector": {
                    "type": "object",
                    "required": ["progress", "uncertainty", "risk_level", "budget", "error_streak", "context_pressure", "side_effect_pressure", "human_dependency"],
                    "properties": {
                        "progress": { "type": "number", "minimum": 0, "maximum": 1 },
                        "uncertainty": { "type": "number", "minimum": 0, "maximum": 1 },
                        "risk_level": { "type": "string", "enum": ["low", "medium", "high"] },
                        "budget": { "$ref": "#/components/schemas/BudgetState" },
                        "error_streak": { "type": "integer", "format": "int32", "minimum": 0 },
                        "context_pressure": { "type": "number", "minimum": 0, "maximum": 1 },
                        "side_effect_pressure": { "type": "number", "minimum": 0, "maximum": 1 },
                        "human_dependency": { "type": "number", "minimum": 0, "maximum": 1 },
                    },
                },
                "TickResponse": {
                    "type": "object",
                    "required": ["session_id", "mode", "state", "events_emitted", "last_sequence"],
                    "properties": {
                        "session_id": { "type": "string", "format": "uuid" },
                        "mode": { "type": "string", "enum": ["explore", "execute", "verify", "recover", "ask_human", "sleep"] },
                        "state": { "$ref": "#/components/schemas/AgentStateVector" },
                        "events_emitted": { "type": "integer", "format": "int64", "minimum": 0 },
                        "last_sequence": { "type": "integer", "format": "int64", "minimum": 0 },
                    },
                },
                "ResolveApprovalRequest": {
                    "type": "object",
                    "required": ["approved"],
                    "properties": {
                        "approved": { "type": "boolean" },
                        "actor": { "type": "string" },
                    },
                },
                "EventRecord": {
                    "type": "object",
                    "description": "Serialized kernel event record",
                    "additionalProperties": true,
                },
                "EventListResponse": {
                    "type": "object",
                    "required": ["session_id", "branch", "from_sequence", "events"],
                    "properties": {
                        "session_id": { "type": "string", "format": "uuid" },
                        "branch": { "type": "string" },
                        "from_sequence": { "type": "integer", "format": "int64", "minimum": 1 },
                        "events": { "type": "array", "items": { "$ref": "#/components/schemas/EventRecord" } },
                    },
                },
                "VoiceStartRequest": {
                    "type": "object",
                    "properties": {
                        "role_prompt": { "type": "string" },
                        "voice_prompt_ref": { "type": "string" },
                        "sample_rate_hz": { "type": "integer", "format": "int32", "minimum": 1 },
                        "channels": { "type": "integer", "format": "int32", "minimum": 1 },
                        "format": { "type": "string" },
                    },
                },
                "VoiceStartResponse": {
                    "type": "object",
                    "required": ["session_id", "voice_session_id", "model", "sample_rate_hz", "channels", "format", "ws_path"],
                    "properties": {
                        "session_id": { "type": "string", "format": "uuid" },
                        "voice_session_id": { "type": "string", "format": "uuid" },
                        "model": { "type": "string" },
                        "sample_rate_hz": { "type": "integer", "format": "int32", "minimum": 1 },
                        "channels": { "type": "integer", "format": "int32", "minimum": 1 },
                        "format": { "type": "string" },
                        "ws_path": { "type": "string" },
                    },
                },
            },
        },
    })
}

pub fn scalar_docs_html(spec_url: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>aiOS API Docs</title>
    <style>
      html, body, #app {{
        margin: 0;
        padding: 0;
        height: 100%;
        width: 100%;
      }}
    </style>
  </head>
  <body>
    <div id="app"></div>
    <script src="https://cdn.jsdelivr.net/npm/@scalar/api-reference"></script>
    <script>
      Scalar.createApiReference('#app', {{
        url: '{spec_url}',
      }});
    </script>
  </body>
</html>
"#
    )
}

#[cfg(test)]
mod tests {
    use super::{openapi_spec, scalar_docs_html};

    #[test]
    fn openapi_spec_declares_31_and_docs_routes() {
        let spec = openapi_spec();
        assert_eq!(spec["openapi"], "3.1.0");
        assert!(spec["paths"]["/openapi.json"].is_object());
        assert!(spec["paths"]["/docs"].is_object());
        assert!(spec["paths"]["/sessions/{session_id}/events/stream/vercel-ai-sdk-v6"].is_object());
        assert!(spec["paths"]["/sessions/{session_id}/branches"].is_object());
        assert!(spec["paths"]["/sessions/{session_id}/branches/{branch_id}/merge"].is_object());
        assert!(spec["components"]["parameters"]["BranchPath"].is_object());
        assert!(spec["components"]["schemas"]["BranchInfo"].is_object());
    }

    #[test]
    fn scalar_html_uses_openapi_url_and_scalar_bundle() {
        let html = scalar_docs_html("/openapi.json");
        assert!(html.contains("https://cdn.jsdelivr.net/npm/@scalar/api-reference"));
        assert!(html.contains("Scalar.createApiReference('#app'"));
        assert!(html.contains("url: '/openapi.json'"));
    }
}
