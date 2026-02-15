use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub type KernelResult<T> = Result<T, KernelError>;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct SessionId(pub Uuid);

impl SessionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct EventId(pub Uuid);

impl EventId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for EventId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ToolRunId(pub Uuid);

impl ToolRunId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ToolRunId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct CheckpointId(pub Uuid);

impl CheckpointId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for CheckpointId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Capability(pub String);

impl Capability {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn fs_read(glob: &str) -> Self {
        Self(format!("fs:read:{glob}"))
    }

    pub fn fs_write(glob: &str) -> Self {
        Self(format!("fs:write:{glob}"))
    }

    pub fn net_egress(host: &str) -> Self {
        Self(format!("net:egress:{host}"))
    }

    pub fn exec(command: &str) -> Self {
        Self(format!("exec:cmd:{command}"))
    }

    pub fn secrets(scope: &str) -> Self {
        Self(format!("secrets:read:{scope}"))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRouting {
    pub primary_model: String,
    pub fallback_models: Vec<String>,
    pub temperature: f32,
}

impl Default for ModelRouting {
    fn default() -> Self {
        Self {
            primary_model: "gpt-5".to_owned(),
            fallback_models: vec!["gpt-4.1".to_owned()],
            temperature: 0.2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicySet {
    pub allow_capabilities: Vec<Capability>,
    pub gate_capabilities: Vec<Capability>,
    pub max_tool_runtime_secs: u64,
    pub max_events_per_turn: u64,
}

impl Default for PolicySet {
    fn default() -> Self {
        Self {
            allow_capabilities: vec![
                Capability::fs_read("/session/**"),
                Capability::fs_write("/session/artifacts/**"),
                Capability::exec("git"),
            ],
            gate_capabilities: vec![Capability::new("payments:initiate")],
            max_tool_runtime_secs: 30,
            max_events_per_turn: 256,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatingMode {
    Explore,
    Execute,
    Verify,
    Recover,
    AskHuman,
    Sleep,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionManifest {
    pub session_id: SessionId,
    pub owner: String,
    pub created_at: DateTime<Utc>,
    pub workspace_root: String,
    pub model_routing: ModelRouting,
    pub policy: PolicySet,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub call_id: Uuid,
    pub tool_name: String,
    pub input: serde_json::Value,
    pub requested_capabilities: Vec<Capability>,
}

impl ToolCall {
    pub fn new(
        tool_name: impl Into<String>,
        input: serde_json::Value,
        requested_capabilities: Vec<Capability>,
    ) -> Self {
        Self {
            call_id: Uuid::new_v4(),
            tool_name: tool_name.into(),
            input,
            requested_capabilities,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ToolOutcome {
    Success { output: serde_json::Value },
    Failure { error: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoopPhase {
    Perceive,
    Deliberate,
    Gate,
    Execute,
    Commit,
    Reflect,
    Sleep,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventKind {
    SessionCreated {
        manifest_hash: String,
    },
    PhaseEntered {
        phase: LoopPhase,
    },
    DeliberationProposed {
        summary: String,
        proposed_tool: Option<ToolCall>,
    },
    ApprovalRequested {
        approval_id: Uuid,
        reason: String,
        capability: Capability,
    },
    ApprovalResolved {
        approval_id: Uuid,
        approved: bool,
        actor: String,
    },
    ToolCallRequested {
        call: ToolCall,
    },
    ToolCallStarted {
        tool_run_id: ToolRunId,
        tool_name: String,
    },
    ToolCallCompleted {
        tool_run_id: ToolRunId,
        exit_status: i32,
        outcome: ToolOutcome,
    },
    VoiceSessionStarted {
        voice_session_id: Uuid,
        adapter: String,
        model: String,
        sample_rate_hz: u32,
        channels: u8,
    },
    VoiceInputChunk {
        voice_session_id: Uuid,
        chunk_index: u64,
        bytes: usize,
        format: String,
    },
    VoiceOutputChunk {
        voice_session_id: Uuid,
        chunk_index: u64,
        bytes: usize,
        format: String,
    },
    VoiceSessionStopped {
        voice_session_id: Uuid,
        reason: String,
    },
    VoiceAdapterError {
        voice_session_id: Uuid,
        message: String,
    },
    FileMutated {
        path: String,
        content_hash: String,
    },
    ObservationExtracted {
        observation_id: Uuid,
    },
    Heartbeat {
        summary: String,
        checkpoint_id: Option<CheckpointId>,
    },
    CheckpointCreated {
        checkpoint_id: CheckpointId,
        event_sequence: u64,
        state_hash: String,
    },
    StateEstimated {
        state: AgentStateVector,
        mode: OperatingMode,
    },
    BudgetUpdated {
        budget: BudgetState,
        reason: String,
    },
    CircuitBreakerTripped {
        reason: String,
        error_streak: u32,
    },
    ErrorRaised {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRecord {
    pub event_id: EventId,
    pub session_id: SessionId,
    pub sequence: u64,
    pub timestamp: DateTime<Utc>,
    pub causation_id: Option<EventId>,
    pub correlation_id: Option<Uuid>,
    pub kind: EventKind,
}

impl EventRecord {
    pub fn new(session_id: SessionId, sequence: u64, kind: EventKind) -> Self {
        Self {
            event_id: EventId::new(),
            session_id,
            sequence,
            timestamp: Utc::now(),
            causation_id: None,
            correlation_id: None,
            kind,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileProvenance {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provenance {
    pub event_start: u64,
    pub event_end: u64,
    pub files: Vec<FileProvenance>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub observation_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub text: String,
    pub tags: Vec<String>,
    pub provenance: Provenance,
}

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
            name: "aiOS kernel agent".to_owned(),
            mission: "Run tool-mediated work safely and reproducibly".to_owned(),
            preferences: IndexMap::new(),
            updated_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointManifest {
    pub checkpoint_id: CheckpointId,
    pub session_id: SessionId,
    pub created_at: DateTime<Utc>,
    pub event_sequence: u64,
    pub state_hash: String,
    pub note: String,
}

#[derive(Debug, Error)]
pub enum KernelError {
    #[error("capability denied: {0}")]
    CapabilityDenied(String),
    #[error("tool not found: {0}")]
    ToolNotFound(String),
    #[error("approval required: {0}")]
    ApprovalRequired(String),
    #[error("io error: {0}")]
    Io(String),
    #[error("serialization error: {0}")]
    Serialization(String),
    #[error("invalid state: {0}")]
    InvalidState(String),
    #[error("runtime error: {0}")]
    Runtime(String),
}
