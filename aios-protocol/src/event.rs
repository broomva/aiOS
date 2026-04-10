//! Canonical event types for the Agent OS.
//!
//! Merges the best of three event models:
//! - Lago's `EventPayload` (35+ variants, forward-compatible deserializer)
//! - Arcan's `AgentEvent` (24 variants, runtime/streaming focused)
//! - aiOS's `EventKind` (40+ variants, homeostasis/voice/phases)
//!
//! Forward-compatible: unknown `"type"` tags deserialize into
//! `Custom { event_type, data }` instead of failing.

use crate::ids::*;
use crate::memory::MemoryScope;
use crate::mode::OperatingMode;
use crate::state::{AgentStateVector, BudgetState, StatePatch};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Event actor identity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActorType {
    User,
    Agent,
    System,
}

/// Event actor metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EventActor {
    #[serde(rename = "type")]
    pub actor_type: ActorType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
}

impl Default for EventActor {
    fn default() -> Self {
        Self {
            actor_type: ActorType::System,
            component: Some("arcan-daemon".to_owned()),
        }
    }
}

/// Event schema descriptor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EventSchema {
    pub name: String,
    pub version: String,
}

impl Default for EventSchema {
    fn default() -> Self {
        Self {
            name: "aios-protocol".to_owned(),
            version: "0.1.0".to_owned(),
        }
    }
}

fn default_agent_id() -> AgentId {
    AgentId::default()
}

/// The universal state-change envelope for the Agent OS.
///
/// Adopts Lago's structure: typed IDs, branch-aware sequencing,
/// causal links, metadata bag, and schema versioning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub event_id: EventId,
    pub session_id: SessionId,
    #[serde(default = "default_agent_id")]
    pub agent_id: AgentId,
    pub branch_id: BranchId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<RunId>,
    pub seq: SeqNo,
    /// Microseconds since UNIX epoch.
    #[serde(rename = "ts_ms", alias = "timestamp")]
    pub timestamp: u64,
    #[serde(default)]
    pub actor: EventActor,
    #[serde(default)]
    pub schema: EventSchema,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "parent_event_id", alias = "parent_id")]
    pub parent_id: Option<EventId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
    pub kind: EventKind,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    #[serde(default = "default_schema_version")]
    pub schema_version: u8,
}

fn default_schema_version() -> u8 {
    1
}

impl EventEnvelope {
    /// Current time in microseconds since UNIX epoch.
    pub fn now_micros() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64
    }
}

/// Convenience event record using `chrono::DateTime<Utc>` timestamps.
///
/// This is the type used by aiOS internal crates. It maps to `EventEnvelope`
/// for storage/streaming but uses ergonomic Rust types for construction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRecord {
    pub event_id: EventId,
    pub session_id: SessionId,
    #[serde(default = "default_agent_id")]
    pub agent_id: AgentId,
    pub branch_id: BranchId,
    pub sequence: SeqNo,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    #[serde(default)]
    pub actor: EventActor,
    #[serde(default)]
    pub schema: EventSchema,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub causation_id: Option<EventId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
    pub kind: EventKind,
}

impl EventRecord {
    /// Create a new event record with the current timestamp.
    pub fn new(
        session_id: SessionId,
        branch_id: BranchId,
        sequence: SeqNo,
        kind: EventKind,
    ) -> Self {
        Self {
            event_id: EventId::default(),
            session_id,
            agent_id: AgentId::default(),
            branch_id,
            sequence,
            timestamp: chrono::Utc::now(),
            actor: EventActor::default(),
            schema: EventSchema::default(),
            causation_id: None,
            correlation_id: None,
            trace_id: None,
            span_id: None,
            digest: None,
            kind,
        }
    }

    /// Convert to the canonical `EventEnvelope` for storage/streaming.
    pub fn to_envelope(&self) -> EventEnvelope {
        EventEnvelope {
            event_id: self.event_id.clone(),
            session_id: self.session_id.clone(),
            agent_id: self.agent_id.clone(),
            branch_id: self.branch_id.clone(),
            run_id: None,
            seq: self.sequence,
            timestamp: self.timestamp.timestamp_micros() as u64,
            actor: self.actor.clone(),
            schema: self.schema.clone(),
            parent_id: self.causation_id.clone(),
            trace_id: self.trace_id.clone(),
            span_id: self.span_id.clone(),
            digest: self.digest.clone(),
            kind: self.kind.clone(),
            metadata: HashMap::new(),
            schema_version: 1,
        }
    }
}

// ─── Canonical EventKind ───────────────────────────────────────────

/// Discriminated union of ALL Agent OS event types.
///
/// This is the canonical taxonomy that all projects (Arcan, Lago, aiOS,
/// Autonomic) must use. Merges ~55 variants from three separate models.
///
/// Forward-compatible: unknown `"type"` tags deserialize into `Custom`.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
#[serde(tag = "type")]
pub enum EventKind {
    // ── Input / sensing ──
    UserMessage {
        content: String,
    },
    ExternalSignal {
        signal_type: String,
        data: serde_json::Value,
    },

    // ── Session lifecycle ──
    SessionCreated {
        name: String,
        config: serde_json::Value,
    },
    SessionResumed {
        #[serde(skip_serializing_if = "Option::is_none")]
        from_snapshot: Option<SnapshotId>,
    },
    SessionClosed {
        reason: String,
    },

    // ── Branch lifecycle ──
    BranchCreated {
        new_branch_id: BranchId,
        fork_point_seq: SeqNo,
        name: String,
    },
    BranchMerged {
        source_branch_id: BranchId,
        merge_seq: SeqNo,
    },

    // ── Loop phases (from aiOS) ──
    PhaseEntered {
        phase: LoopPhase,
    },
    DeliberationProposed {
        summary: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        proposed_tool: Option<String>,
    },

    // ── Run lifecycle (from Lago + Arcan) ──
    RunStarted {
        provider: String,
        max_iterations: u32,
    },
    RunFinished {
        reason: String,
        total_iterations: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        final_answer: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<TokenUsage>,
    },
    RunErrored {
        error: String,
    },

    // ── Step lifecycle (from Lago) ──
    StepStarted {
        index: u32,
    },
    StepFinished {
        index: u32,
        stop_reason: String,
        directive_count: usize,
    },

    // ── Text streaming (from Arcan + Lago) ──
    AssistantTextDelta {
        delta: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        index: Option<u32>,
    },
    AssistantMessageCommitted {
        role: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        token_usage: Option<TokenUsage>,
    },
    TextDelta {
        delta: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        index: Option<u32>,
    },
    Message {
        role: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        token_usage: Option<TokenUsage>,
    },

    // ── Tool lifecycle (merged from all three) ──
    ToolCallRequested {
        call_id: String,
        tool_name: String,
        arguments: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        category: Option<String>,
    },
    ToolCallStarted {
        tool_run_id: ToolRunId,
        tool_name: String,
    },
    ToolCallCompleted {
        tool_run_id: ToolRunId,
        #[serde(skip_serializing_if = "Option::is_none")]
        call_id: Option<String>,
        tool_name: String,
        result: serde_json::Value,
        duration_ms: u64,
        status: SpanStatus,
    },
    ToolCallFailed {
        call_id: String,
        tool_name: String,
        error: String,
    },

    // ── Knowledge operations ──
    /// Agent searched the knowledge graph.
    KnowledgeSearched {
        /// The search query.
        query: String,
        /// Number of results returned.
        result_count: u32,
        /// Highest relevance score among results.
        top_relevance: f64,
        /// Search duration in milliseconds.
        duration_ms: u64,
    },
    /// Knowledge context was injected into the agent prompt.
    KnowledgeRetrieved {
        /// Number of notes injected into context.
        note_count: u32,
        /// Estimated token count of injected context.
        context_tokens: u32,
        /// Source of the injected knowledge.
        source: String,
    },
    /// Knowledge quality was evaluated.
    KnowledgeEvaluated {
        /// Aggregate health score (0.0-1.0).
        health_score: f32,
        /// Total notes in the knowledge index.
        note_count: u32,
        /// Number of detected contradictions.
        contradictions: u32,
        /// Number of referenced but missing pages.
        missing_pages: u32,
        /// Number of orphan pages.
        orphans: u32,
    },

    // ── File operations (from Lago) ──
    FileWrite {
        path: String,
        blob_hash: BlobHash,
        size_bytes: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        content_type: Option<String>,
    },
    FileDelete {
        path: String,
    },
    FileRename {
        old_path: String,
        new_path: String,
    },
    FileMutated {
        path: String,
        content_hash: String,
    },

    // ── State management (from Lago + Arcan) ──
    StatePatchCommitted {
        new_version: u64,
        patch: StatePatch,
    },
    StatePatched {
        #[serde(skip_serializing_if = "Option::is_none")]
        index: Option<u32>,
        patch: serde_json::Value,
        revision: u64,
    },
    ContextCompacted {
        dropped_count: usize,
        tokens_before: usize,
        tokens_after: usize,
    },

    // ── Policy (from Lago) ──
    PolicyEvaluated {
        tool_name: String,
        decision: PolicyDecisionKind,
        #[serde(skip_serializing_if = "Option::is_none")]
        rule_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        explanation: Option<String>,
    },

    // ── Approval gate (from Lago + Arcan + aiOS) ──
    ApprovalRequested {
        approval_id: ApprovalId,
        call_id: String,
        tool_name: String,
        arguments: serde_json::Value,
        risk: RiskLevel,
    },
    ApprovalResolved {
        approval_id: ApprovalId,
        decision: ApprovalDecision,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },

    // ── Snapshots (from Lago) ──
    SnapshotCreated {
        snapshot_id: SnapshotId,
        snapshot_type: SnapshotType,
        covers_through_seq: SeqNo,
        data_hash: BlobHash,
    },

    // ── Sandbox lifecycle (from Lago) ──
    SandboxCreated {
        sandbox_id: String,
        tier: String,
        config: serde_json::Value,
    },
    SandboxExecuted {
        sandbox_id: String,
        command: String,
        exit_code: i32,
        duration_ms: u64,
    },
    SandboxViolation {
        sandbox_id: String,
        violation_type: String,
        details: String,
    },
    SandboxDestroyed {
        sandbox_id: String,
    },

    // ── Memory (from Lago) ──
    ObservationAppended {
        scope: MemoryScope,
        observation_ref: BlobHash,
        #[serde(skip_serializing_if = "Option::is_none")]
        source_run_id: Option<String>,
    },
    ReflectionCompacted {
        scope: MemoryScope,
        summary_ref: BlobHash,
        covers_through_seq: SeqNo,
    },
    MemoryProposed {
        scope: MemoryScope,
        proposal_id: MemoryId,
        entries_ref: BlobHash,
        #[serde(skip_serializing_if = "Option::is_none")]
        source_run_id: Option<String>,
    },
    MemoryCommitted {
        scope: MemoryScope,
        memory_id: MemoryId,
        committed_ref: BlobHash,
        #[serde(skip_serializing_if = "Option::is_none")]
        supersedes: Option<MemoryId>,
    },
    MemoryTombstoned {
        scope: MemoryScope,
        memory_id: MemoryId,
        reason: String,
    },

    // ── Homeostasis (from aiOS) ──
    Heartbeat {
        summary: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        checkpoint_id: Option<CheckpointId>,
    },
    StateEstimated {
        state: AgentStateVector,
        mode: OperatingMode,
    },
    BudgetUpdated {
        budget: BudgetState,
        reason: String,
    },
    ModeChanged {
        from: OperatingMode,
        to: OperatingMode,
        reason: String,
    },
    GatesUpdated {
        gates: serde_json::Value,
        reason: String,
    },
    CircuitBreakerTripped {
        reason: String,
        error_streak: u32,
    },

    // ── Checkpoints (from aiOS) ──
    CheckpointCreated {
        checkpoint_id: CheckpointId,
        event_sequence: u64,
        state_hash: String,
    },
    CheckpointRestored {
        checkpoint_id: CheckpointId,
        restored_to_seq: u64,
    },

    // ── Voice (from aiOS) ──
    VoiceSessionStarted {
        voice_session_id: String,
        adapter: String,
        model: String,
        sample_rate_hz: u32,
        channels: u8,
    },
    VoiceInputChunk {
        voice_session_id: String,
        chunk_index: u64,
        bytes: usize,
        format: String,
    },
    VoiceOutputChunk {
        voice_session_id: String,
        chunk_index: u64,
        bytes: usize,
        format: String,
    },
    VoiceSessionStopped {
        voice_session_id: String,
        reason: String,
    },
    VoiceAdapterError {
        voice_session_id: String,
        message: String,
    },

    // ── World models (new, forward-looking) ──
    WorldModelObserved {
        state_ref: BlobHash,
        meta: serde_json::Value,
    },
    WorldModelRollout {
        trajectory_ref: BlobHash,
        #[serde(skip_serializing_if = "Option::is_none")]
        score: Option<f32>,
    },

    // ── Intent lifecycle (new) ──
    IntentProposed {
        intent_id: String,
        kind: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        risk: Option<RiskLevel>,
    },
    IntentEvaluated {
        intent_id: String,
        allowed: bool,
        requires_approval: bool,
        #[serde(default)]
        reasons: Vec<String>,
    },
    IntentApproved {
        intent_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        actor: Option<String>,
    },
    IntentRejected {
        intent_id: String,
        #[serde(default)]
        reasons: Vec<String>,
    },

    // ── Hive collaborative evolution ──
    HiveTaskCreated {
        hive_task_id: HiveTaskId,
        objective: String,
        agent_count: u32,
    },
    HiveArtifactShared {
        hive_task_id: HiveTaskId,
        source_session_id: SessionId,
        score: f32,
        mutation_summary: String,
    },
    HiveSelectionMade {
        hive_task_id: HiveTaskId,
        winning_session_id: SessionId,
        winning_score: f32,
        generation: u32,
    },
    HiveGenerationCompleted {
        hive_task_id: HiveTaskId,
        generation: u32,
        best_score: f32,
        agent_results: serde_json::Value,
    },
    HiveTaskCompleted {
        hive_task_id: HiveTaskId,
        total_generations: u32,
        total_trials: u32,
        final_score: f32,
    },

    // ── Queue & steering (Phase 2.5) ──
    Queued {
        queue_id: String,
        mode: SteeringMode,
        message: String,
    },
    Steered {
        queue_id: String,
        /// Tool boundary where preemption occurred (e.g. "tool:read_file:call-3").
        preempted_at: String,
    },
    QueueDrained {
        queue_id: String,
        processed: usize,
    },

    // ── Error ──
    ErrorRaised {
        message: String,
    },

    // ── Forward-compatible catch-all ──
    Custom {
        event_type: String,
        data: serde_json::Value,
    },
}

impl EventKind {
    /// Returns the PascalCase variant name as a static string.
    ///
    /// Useful for telemetry span attributes, log fields, and journal indexing
    /// without the overhead of serialization.
    #[allow(clippy::too_many_lines)]
    pub fn variant_name(&self) -> &'static str {
        match self {
            Self::UserMessage { .. } => "UserMessage",
            Self::ExternalSignal { .. } => "ExternalSignal",
            Self::SessionCreated { .. } => "SessionCreated",
            Self::SessionResumed { .. } => "SessionResumed",
            Self::SessionClosed { .. } => "SessionClosed",
            Self::BranchCreated { .. } => "BranchCreated",
            Self::BranchMerged { .. } => "BranchMerged",
            Self::PhaseEntered { .. } => "PhaseEntered",
            Self::DeliberationProposed { .. } => "DeliberationProposed",
            Self::RunStarted { .. } => "RunStarted",
            Self::RunFinished { .. } => "RunFinished",
            Self::RunErrored { .. } => "RunErrored",
            Self::StepStarted { .. } => "StepStarted",
            Self::StepFinished { .. } => "StepFinished",
            Self::AssistantTextDelta { .. } => "AssistantTextDelta",
            Self::AssistantMessageCommitted { .. } => "AssistantMessageCommitted",
            Self::TextDelta { .. } => "TextDelta",
            Self::Message { .. } => "Message",
            Self::ToolCallRequested { .. } => "ToolCallRequested",
            Self::ToolCallStarted { .. } => "ToolCallStarted",
            Self::ToolCallCompleted { .. } => "ToolCallCompleted",
            Self::ToolCallFailed { .. } => "ToolCallFailed",
            Self::KnowledgeSearched { .. } => "KnowledgeSearched",
            Self::KnowledgeRetrieved { .. } => "KnowledgeRetrieved",
            Self::KnowledgeEvaluated { .. } => "KnowledgeEvaluated",
            Self::FileWrite { .. } => "FileWrite",
            Self::FileDelete { .. } => "FileDelete",
            Self::FileRename { .. } => "FileRename",
            Self::FileMutated { .. } => "FileMutated",
            Self::StatePatchCommitted { .. } => "StatePatchCommitted",
            Self::StatePatched { .. } => "StatePatched",
            Self::ContextCompacted { .. } => "ContextCompacted",
            Self::PolicyEvaluated { .. } => "PolicyEvaluated",
            Self::ApprovalRequested { .. } => "ApprovalRequested",
            Self::ApprovalResolved { .. } => "ApprovalResolved",
            Self::SnapshotCreated { .. } => "SnapshotCreated",
            Self::SandboxCreated { .. } => "SandboxCreated",
            Self::SandboxExecuted { .. } => "SandboxExecuted",
            Self::SandboxViolation { .. } => "SandboxViolation",
            Self::SandboxDestroyed { .. } => "SandboxDestroyed",
            Self::ObservationAppended { .. } => "ObservationAppended",
            Self::ReflectionCompacted { .. } => "ReflectionCompacted",
            Self::MemoryProposed { .. } => "MemoryProposed",
            Self::MemoryCommitted { .. } => "MemoryCommitted",
            Self::MemoryTombstoned { .. } => "MemoryTombstoned",
            Self::Heartbeat { .. } => "Heartbeat",
            Self::StateEstimated { .. } => "StateEstimated",
            Self::BudgetUpdated { .. } => "BudgetUpdated",
            Self::ModeChanged { .. } => "ModeChanged",
            Self::GatesUpdated { .. } => "GatesUpdated",
            Self::CircuitBreakerTripped { .. } => "CircuitBreakerTripped",
            Self::CheckpointCreated { .. } => "CheckpointCreated",
            Self::CheckpointRestored { .. } => "CheckpointRestored",
            Self::VoiceSessionStarted { .. } => "VoiceSessionStarted",
            Self::VoiceInputChunk { .. } => "VoiceInputChunk",
            Self::VoiceOutputChunk { .. } => "VoiceOutputChunk",
            Self::VoiceSessionStopped { .. } => "VoiceSessionStopped",
            Self::VoiceAdapterError { .. } => "VoiceAdapterError",
            Self::WorldModelObserved { .. } => "WorldModelObserved",
            Self::WorldModelRollout { .. } => "WorldModelRollout",
            Self::IntentProposed { .. } => "IntentProposed",
            Self::IntentEvaluated { .. } => "IntentEvaluated",
            Self::IntentApproved { .. } => "IntentApproved",
            Self::IntentRejected { .. } => "IntentRejected",
            Self::HiveTaskCreated { .. } => "HiveTaskCreated",
            Self::HiveArtifactShared { .. } => "HiveArtifactShared",
            Self::HiveSelectionMade { .. } => "HiveSelectionMade",
            Self::HiveGenerationCompleted { .. } => "HiveGenerationCompleted",
            Self::HiveTaskCompleted { .. } => "HiveTaskCompleted",
            Self::Queued { .. } => "Queued",
            Self::Steered { .. } => "Steered",
            Self::QueueDrained { .. } => "QueueDrained",
            Self::ErrorRaised { .. } => "ErrorRaised",
            Self::Custom { .. } => "Custom",
        }
    }
}

// ─── Supporting types ──────────────────────────────────────────────

/// Agent loop phase (from aiOS).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

/// Token usage reported by LLM providers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenUsage {
    #[serde(default)]
    pub prompt_tokens: u32,
    #[serde(default)]
    pub completion_tokens: u32,
    #[serde(default)]
    pub total_tokens: u32,
}

/// Tool execution span status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpanStatus {
    Ok,
    Error,
    Timeout,
    Cancelled,
}

/// Risk level for policy evaluation. Includes Critical from Lago.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

/// Approval decision outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    Approved,
    Denied,
    Timeout,
}

/// Snapshot type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotType {
    Full,
    Incremental,
}

/// Policy decision kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyDecisionKind {
    Allow,
    Deny,
    RequireApproval,
}

/// Steering mode for queued messages (Phase 2.5).
///
/// Determines how a queued message interacts with an active run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SteeringMode {
    /// Queue message for processing after current run completes.
    Collect,
    /// Redirect agent at next tool boundary (safe preemption).
    Steer,
    /// Queue as follow-up to current run (same context).
    Followup,
    /// Interrupt at next safe point (tool boundary), highest priority.
    Interrupt,
}

// ─── Forward-compatible deserializer ───────────────────────────────

/// Internal helper enum for the forward-compatible deserializer.
/// Mirrors EventKind exactly but derives Deserialize.
#[derive(Deserialize)]
#[serde(tag = "type")]
enum EventKindKnown {
    UserMessage {
        content: String,
    },
    ExternalSignal {
        signal_type: String,
        data: serde_json::Value,
    },
    SessionCreated {
        name: String,
        config: serde_json::Value,
    },
    SessionResumed {
        #[serde(default)]
        from_snapshot: Option<SnapshotId>,
    },
    SessionClosed {
        reason: String,
    },
    BranchCreated {
        new_branch_id: BranchId,
        fork_point_seq: SeqNo,
        name: String,
    },
    BranchMerged {
        source_branch_id: BranchId,
        merge_seq: SeqNo,
    },
    PhaseEntered {
        phase: LoopPhase,
    },
    DeliberationProposed {
        summary: String,
        #[serde(default)]
        proposed_tool: Option<String>,
    },
    RunStarted {
        provider: String,
        max_iterations: u32,
    },
    RunFinished {
        reason: String,
        total_iterations: u32,
        #[serde(default)]
        final_answer: Option<String>,
        #[serde(default)]
        usage: Option<TokenUsage>,
    },
    RunErrored {
        error: String,
    },
    StepStarted {
        index: u32,
    },
    StepFinished {
        index: u32,
        stop_reason: String,
        directive_count: usize,
    },
    AssistantTextDelta {
        delta: String,
        #[serde(default)]
        index: Option<u32>,
    },
    AssistantMessageCommitted {
        role: String,
        content: String,
        #[serde(default)]
        model: Option<String>,
        #[serde(default)]
        token_usage: Option<TokenUsage>,
    },
    TextDelta {
        delta: String,
        #[serde(default)]
        index: Option<u32>,
    },
    Message {
        role: String,
        content: String,
        #[serde(default)]
        model: Option<String>,
        #[serde(default)]
        token_usage: Option<TokenUsage>,
    },
    ToolCallRequested {
        call_id: String,
        tool_name: String,
        arguments: serde_json::Value,
        #[serde(default)]
        category: Option<String>,
    },
    ToolCallStarted {
        tool_run_id: ToolRunId,
        tool_name: String,
    },
    ToolCallCompleted {
        tool_run_id: ToolRunId,
        #[serde(default)]
        call_id: Option<String>,
        tool_name: String,
        result: serde_json::Value,
        duration_ms: u64,
        status: SpanStatus,
    },
    ToolCallFailed {
        call_id: String,
        tool_name: String,
        error: String,
    },
    KnowledgeSearched {
        query: String,
        result_count: u32,
        top_relevance: f64,
        duration_ms: u64,
    },
    KnowledgeRetrieved {
        note_count: u32,
        context_tokens: u32,
        source: String,
    },
    KnowledgeEvaluated {
        health_score: f32,
        note_count: u32,
        contradictions: u32,
        missing_pages: u32,
        orphans: u32,
    },
    FileWrite {
        path: String,
        blob_hash: BlobHash,
        size_bytes: u64,
        #[serde(default)]
        content_type: Option<String>,
    },
    FileDelete {
        path: String,
    },
    FileRename {
        old_path: String,
        new_path: String,
    },
    FileMutated {
        path: String,
        content_hash: String,
    },
    StatePatched {
        #[serde(default)]
        index: Option<u32>,
        patch: serde_json::Value,
        revision: u64,
    },
    StatePatchCommitted {
        new_version: u64,
        patch: StatePatch,
    },
    ContextCompacted {
        dropped_count: usize,
        tokens_before: usize,
        tokens_after: usize,
    },
    PolicyEvaluated {
        tool_name: String,
        decision: PolicyDecisionKind,
        #[serde(default)]
        rule_id: Option<String>,
        #[serde(default)]
        explanation: Option<String>,
    },
    ApprovalRequested {
        approval_id: ApprovalId,
        call_id: String,
        tool_name: String,
        arguments: serde_json::Value,
        risk: RiskLevel,
    },
    ApprovalResolved {
        approval_id: ApprovalId,
        decision: ApprovalDecision,
        #[serde(default)]
        reason: Option<String>,
    },
    SnapshotCreated {
        snapshot_id: SnapshotId,
        snapshot_type: SnapshotType,
        covers_through_seq: SeqNo,
        data_hash: BlobHash,
    },
    SandboxCreated {
        sandbox_id: String,
        tier: String,
        config: serde_json::Value,
    },
    SandboxExecuted {
        sandbox_id: String,
        command: String,
        exit_code: i32,
        duration_ms: u64,
    },
    SandboxViolation {
        sandbox_id: String,
        violation_type: String,
        details: String,
    },
    SandboxDestroyed {
        sandbox_id: String,
    },
    ObservationAppended {
        scope: MemoryScope,
        observation_ref: BlobHash,
        #[serde(default)]
        source_run_id: Option<String>,
    },
    ReflectionCompacted {
        scope: MemoryScope,
        summary_ref: BlobHash,
        covers_through_seq: SeqNo,
    },
    MemoryProposed {
        scope: MemoryScope,
        proposal_id: MemoryId,
        entries_ref: BlobHash,
        #[serde(default)]
        source_run_id: Option<String>,
    },
    MemoryCommitted {
        scope: MemoryScope,
        memory_id: MemoryId,
        committed_ref: BlobHash,
        #[serde(default)]
        supersedes: Option<MemoryId>,
    },
    MemoryTombstoned {
        scope: MemoryScope,
        memory_id: MemoryId,
        reason: String,
    },
    Heartbeat {
        summary: String,
        #[serde(default)]
        checkpoint_id: Option<CheckpointId>,
    },
    StateEstimated {
        state: AgentStateVector,
        mode: OperatingMode,
    },
    BudgetUpdated {
        budget: BudgetState,
        reason: String,
    },
    ModeChanged {
        from: OperatingMode,
        to: OperatingMode,
        reason: String,
    },
    GatesUpdated {
        gates: serde_json::Value,
        reason: String,
    },
    CircuitBreakerTripped {
        reason: String,
        error_streak: u32,
    },
    CheckpointCreated {
        checkpoint_id: CheckpointId,
        event_sequence: u64,
        state_hash: String,
    },
    CheckpointRestored {
        checkpoint_id: CheckpointId,
        restored_to_seq: u64,
    },
    VoiceSessionStarted {
        voice_session_id: String,
        adapter: String,
        model: String,
        sample_rate_hz: u32,
        channels: u8,
    },
    VoiceInputChunk {
        voice_session_id: String,
        chunk_index: u64,
        bytes: usize,
        format: String,
    },
    VoiceOutputChunk {
        voice_session_id: String,
        chunk_index: u64,
        bytes: usize,
        format: String,
    },
    VoiceSessionStopped {
        voice_session_id: String,
        reason: String,
    },
    VoiceAdapterError {
        voice_session_id: String,
        message: String,
    },
    WorldModelObserved {
        state_ref: BlobHash,
        meta: serde_json::Value,
    },
    WorldModelRollout {
        trajectory_ref: BlobHash,
        #[serde(default)]
        score: Option<f32>,
    },
    IntentProposed {
        intent_id: String,
        kind: String,
        #[serde(default)]
        risk: Option<RiskLevel>,
    },
    IntentEvaluated {
        intent_id: String,
        allowed: bool,
        requires_approval: bool,
        #[serde(default)]
        reasons: Vec<String>,
    },
    IntentApproved {
        intent_id: String,
        #[serde(default)]
        actor: Option<String>,
    },
    IntentRejected {
        intent_id: String,
        #[serde(default)]
        reasons: Vec<String>,
    },
    HiveTaskCreated {
        hive_task_id: HiveTaskId,
        objective: String,
        agent_count: u32,
    },
    HiveArtifactShared {
        hive_task_id: HiveTaskId,
        source_session_id: SessionId,
        score: f32,
        mutation_summary: String,
    },
    HiveSelectionMade {
        hive_task_id: HiveTaskId,
        winning_session_id: SessionId,
        winning_score: f32,
        generation: u32,
    },
    HiveGenerationCompleted {
        hive_task_id: HiveTaskId,
        generation: u32,
        best_score: f32,
        agent_results: serde_json::Value,
    },
    HiveTaskCompleted {
        hive_task_id: HiveTaskId,
        total_generations: u32,
        total_trials: u32,
        final_score: f32,
    },
    Queued {
        queue_id: String,
        mode: SteeringMode,
        message: String,
    },
    Steered {
        queue_id: String,
        preempted_at: String,
    },
    QueueDrained {
        queue_id: String,
        processed: usize,
    },
    ErrorRaised {
        message: String,
    },
    Custom {
        event_type: String,
        data: serde_json::Value,
    },
}

/// Forward-compatible deserializer: unknown variants become `Custom`.
impl<'de> Deserialize<'de> for EventKind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = serde_json::Value::deserialize(deserializer)?;
        match serde_json::from_value::<EventKindKnown>(raw.clone()) {
            Ok(known) => Ok(known.into()),
            Err(_) => {
                let event_type = raw
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown")
                    .to_string();
                let mut data = raw;
                if let Some(obj) = data.as_object_mut() {
                    obj.remove("type");
                }
                Ok(EventKind::Custom { event_type, data })
            }
        }
    }
}

/// Conversion from the known helper enum to the public EventKind.
/// This is mechanical — each variant maps 1:1.
impl From<EventKindKnown> for EventKind {
    #[allow(clippy::too_many_lines)]
    fn from(k: EventKindKnown) -> Self {
        match k {
            EventKindKnown::UserMessage { content } => Self::UserMessage { content },
            EventKindKnown::ExternalSignal { signal_type, data } => {
                Self::ExternalSignal { signal_type, data }
            }
            EventKindKnown::SessionCreated { name, config } => {
                Self::SessionCreated { name, config }
            }
            EventKindKnown::SessionResumed { from_snapshot } => {
                Self::SessionResumed { from_snapshot }
            }
            EventKindKnown::SessionClosed { reason } => Self::SessionClosed { reason },
            EventKindKnown::BranchCreated {
                new_branch_id,
                fork_point_seq,
                name,
            } => Self::BranchCreated {
                new_branch_id,
                fork_point_seq,
                name,
            },
            EventKindKnown::BranchMerged {
                source_branch_id,
                merge_seq,
            } => Self::BranchMerged {
                source_branch_id,
                merge_seq,
            },
            EventKindKnown::PhaseEntered { phase } => Self::PhaseEntered { phase },
            EventKindKnown::DeliberationProposed {
                summary,
                proposed_tool,
            } => Self::DeliberationProposed {
                summary,
                proposed_tool,
            },
            EventKindKnown::RunStarted {
                provider,
                max_iterations,
            } => Self::RunStarted {
                provider,
                max_iterations,
            },
            EventKindKnown::RunFinished {
                reason,
                total_iterations,
                final_answer,
                usage,
            } => Self::RunFinished {
                reason,
                total_iterations,
                final_answer,
                usage,
            },
            EventKindKnown::RunErrored { error } => Self::RunErrored { error },
            EventKindKnown::StepStarted { index } => Self::StepStarted { index },
            EventKindKnown::StepFinished {
                index,
                stop_reason,
                directive_count,
            } => Self::StepFinished {
                index,
                stop_reason,
                directive_count,
            },
            EventKindKnown::AssistantTextDelta { delta, index } => {
                Self::AssistantTextDelta { delta, index }
            }
            EventKindKnown::AssistantMessageCommitted {
                role,
                content,
                model,
                token_usage,
            } => Self::AssistantMessageCommitted {
                role,
                content,
                model,
                token_usage,
            },
            EventKindKnown::TextDelta { delta, index } => Self::TextDelta { delta, index },
            EventKindKnown::Message {
                role,
                content,
                model,
                token_usage,
            } => Self::Message {
                role,
                content,
                model,
                token_usage,
            },
            EventKindKnown::ToolCallRequested {
                call_id,
                tool_name,
                arguments,
                category,
            } => Self::ToolCallRequested {
                call_id,
                tool_name,
                arguments,
                category,
            },
            EventKindKnown::ToolCallStarted {
                tool_run_id,
                tool_name,
            } => Self::ToolCallStarted {
                tool_run_id,
                tool_name,
            },
            EventKindKnown::ToolCallCompleted {
                tool_run_id,
                call_id,
                tool_name,
                result,
                duration_ms,
                status,
            } => Self::ToolCallCompleted {
                tool_run_id,
                call_id,
                tool_name,
                result,
                duration_ms,
                status,
            },
            EventKindKnown::ToolCallFailed {
                call_id,
                tool_name,
                error,
            } => Self::ToolCallFailed {
                call_id,
                tool_name,
                error,
            },
            EventKindKnown::KnowledgeSearched {
                query,
                result_count,
                top_relevance,
                duration_ms,
            } => Self::KnowledgeSearched {
                query,
                result_count,
                top_relevance,
                duration_ms,
            },
            EventKindKnown::KnowledgeRetrieved {
                note_count,
                context_tokens,
                source,
            } => Self::KnowledgeRetrieved {
                note_count,
                context_tokens,
                source,
            },
            EventKindKnown::KnowledgeEvaluated {
                health_score,
                note_count,
                contradictions,
                missing_pages,
                orphans,
            } => Self::KnowledgeEvaluated {
                health_score,
                note_count,
                contradictions,
                missing_pages,
                orphans,
            },
            EventKindKnown::FileWrite {
                path,
                blob_hash,
                size_bytes,
                content_type,
            } => Self::FileWrite {
                path,
                blob_hash,
                size_bytes,
                content_type,
            },
            EventKindKnown::FileDelete { path } => Self::FileDelete { path },
            EventKindKnown::FileRename { old_path, new_path } => {
                Self::FileRename { old_path, new_path }
            }
            EventKindKnown::FileMutated { path, content_hash } => {
                Self::FileMutated { path, content_hash }
            }
            EventKindKnown::StatePatched {
                index,
                patch,
                revision,
            } => Self::StatePatched {
                index,
                patch,
                revision,
            },
            EventKindKnown::StatePatchCommitted { new_version, patch } => {
                Self::StatePatchCommitted { new_version, patch }
            }
            EventKindKnown::ContextCompacted {
                dropped_count,
                tokens_before,
                tokens_after,
            } => Self::ContextCompacted {
                dropped_count,
                tokens_before,
                tokens_after,
            },
            EventKindKnown::PolicyEvaluated {
                tool_name,
                decision,
                rule_id,
                explanation,
            } => Self::PolicyEvaluated {
                tool_name,
                decision,
                rule_id,
                explanation,
            },
            EventKindKnown::ApprovalRequested {
                approval_id,
                call_id,
                tool_name,
                arguments,
                risk,
            } => Self::ApprovalRequested {
                approval_id,
                call_id,
                tool_name,
                arguments,
                risk,
            },
            EventKindKnown::ApprovalResolved {
                approval_id,
                decision,
                reason,
            } => Self::ApprovalResolved {
                approval_id,
                decision,
                reason,
            },
            EventKindKnown::SnapshotCreated {
                snapshot_id,
                snapshot_type,
                covers_through_seq,
                data_hash,
            } => Self::SnapshotCreated {
                snapshot_id,
                snapshot_type,
                covers_through_seq,
                data_hash,
            },
            EventKindKnown::SandboxCreated {
                sandbox_id,
                tier,
                config,
            } => Self::SandboxCreated {
                sandbox_id,
                tier,
                config,
            },
            EventKindKnown::SandboxExecuted {
                sandbox_id,
                command,
                exit_code,
                duration_ms,
            } => Self::SandboxExecuted {
                sandbox_id,
                command,
                exit_code,
                duration_ms,
            },
            EventKindKnown::SandboxViolation {
                sandbox_id,
                violation_type,
                details,
            } => Self::SandboxViolation {
                sandbox_id,
                violation_type,
                details,
            },
            EventKindKnown::SandboxDestroyed { sandbox_id } => {
                Self::SandboxDestroyed { sandbox_id }
            }
            EventKindKnown::ObservationAppended {
                scope,
                observation_ref,
                source_run_id,
            } => Self::ObservationAppended {
                scope,
                observation_ref,
                source_run_id,
            },
            EventKindKnown::ReflectionCompacted {
                scope,
                summary_ref,
                covers_through_seq,
            } => Self::ReflectionCompacted {
                scope,
                summary_ref,
                covers_through_seq,
            },
            EventKindKnown::MemoryProposed {
                scope,
                proposal_id,
                entries_ref,
                source_run_id,
            } => Self::MemoryProposed {
                scope,
                proposal_id,
                entries_ref,
                source_run_id,
            },
            EventKindKnown::MemoryCommitted {
                scope,
                memory_id,
                committed_ref,
                supersedes,
            } => Self::MemoryCommitted {
                scope,
                memory_id,
                committed_ref,
                supersedes,
            },
            EventKindKnown::MemoryTombstoned {
                scope,
                memory_id,
                reason,
            } => Self::MemoryTombstoned {
                scope,
                memory_id,
                reason,
            },
            EventKindKnown::Heartbeat {
                summary,
                checkpoint_id,
            } => Self::Heartbeat {
                summary,
                checkpoint_id,
            },
            EventKindKnown::StateEstimated { state, mode } => Self::StateEstimated { state, mode },
            EventKindKnown::BudgetUpdated { budget, reason } => {
                Self::BudgetUpdated { budget, reason }
            }
            EventKindKnown::ModeChanged { from, to, reason } => {
                Self::ModeChanged { from, to, reason }
            }
            EventKindKnown::GatesUpdated { gates, reason } => Self::GatesUpdated { gates, reason },
            EventKindKnown::CircuitBreakerTripped {
                reason,
                error_streak,
            } => Self::CircuitBreakerTripped {
                reason,
                error_streak,
            },
            EventKindKnown::CheckpointCreated {
                checkpoint_id,
                event_sequence,
                state_hash,
            } => Self::CheckpointCreated {
                checkpoint_id,
                event_sequence,
                state_hash,
            },
            EventKindKnown::CheckpointRestored {
                checkpoint_id,
                restored_to_seq,
            } => Self::CheckpointRestored {
                checkpoint_id,
                restored_to_seq,
            },
            EventKindKnown::VoiceSessionStarted {
                voice_session_id,
                adapter,
                model,
                sample_rate_hz,
                channels,
            } => Self::VoiceSessionStarted {
                voice_session_id,
                adapter,
                model,
                sample_rate_hz,
                channels,
            },
            EventKindKnown::VoiceInputChunk {
                voice_session_id,
                chunk_index,
                bytes,
                format,
            } => Self::VoiceInputChunk {
                voice_session_id,
                chunk_index,
                bytes,
                format,
            },
            EventKindKnown::VoiceOutputChunk {
                voice_session_id,
                chunk_index,
                bytes,
                format,
            } => Self::VoiceOutputChunk {
                voice_session_id,
                chunk_index,
                bytes,
                format,
            },
            EventKindKnown::VoiceSessionStopped {
                voice_session_id,
                reason,
            } => Self::VoiceSessionStopped {
                voice_session_id,
                reason,
            },
            EventKindKnown::VoiceAdapterError {
                voice_session_id,
                message,
            } => Self::VoiceAdapterError {
                voice_session_id,
                message,
            },
            EventKindKnown::WorldModelObserved { state_ref, meta } => {
                Self::WorldModelObserved { state_ref, meta }
            }
            EventKindKnown::WorldModelRollout {
                trajectory_ref,
                score,
            } => Self::WorldModelRollout {
                trajectory_ref,
                score,
            },
            EventKindKnown::IntentProposed {
                intent_id,
                kind,
                risk,
            } => Self::IntentProposed {
                intent_id,
                kind,
                risk,
            },
            EventKindKnown::IntentEvaluated {
                intent_id,
                allowed,
                requires_approval,
                reasons,
            } => Self::IntentEvaluated {
                intent_id,
                allowed,
                requires_approval,
                reasons,
            },
            EventKindKnown::IntentApproved { intent_id, actor } => {
                Self::IntentApproved { intent_id, actor }
            }
            EventKindKnown::IntentRejected { intent_id, reasons } => {
                Self::IntentRejected { intent_id, reasons }
            }
            EventKindKnown::HiveTaskCreated {
                hive_task_id,
                objective,
                agent_count,
            } => Self::HiveTaskCreated {
                hive_task_id,
                objective,
                agent_count,
            },
            EventKindKnown::HiveArtifactShared {
                hive_task_id,
                source_session_id,
                score,
                mutation_summary,
            } => Self::HiveArtifactShared {
                hive_task_id,
                source_session_id,
                score,
                mutation_summary,
            },
            EventKindKnown::HiveSelectionMade {
                hive_task_id,
                winning_session_id,
                winning_score,
                generation,
            } => Self::HiveSelectionMade {
                hive_task_id,
                winning_session_id,
                winning_score,
                generation,
            },
            EventKindKnown::HiveGenerationCompleted {
                hive_task_id,
                generation,
                best_score,
                agent_results,
            } => Self::HiveGenerationCompleted {
                hive_task_id,
                generation,
                best_score,
                agent_results,
            },
            EventKindKnown::HiveTaskCompleted {
                hive_task_id,
                total_generations,
                total_trials,
                final_score,
            } => Self::HiveTaskCompleted {
                hive_task_id,
                total_generations,
                total_trials,
                final_score,
            },
            EventKindKnown::Queued {
                queue_id,
                mode,
                message,
            } => Self::Queued {
                queue_id,
                mode,
                message,
            },
            EventKindKnown::Steered {
                queue_id,
                preempted_at,
            } => Self::Steered {
                queue_id,
                preempted_at,
            },
            EventKindKnown::QueueDrained {
                queue_id,
                processed,
            } => Self::QueueDrained {
                queue_id,
                processed,
            },
            EventKindKnown::ErrorRaised { message } => Self::ErrorRaised { message },
            EventKindKnown::Custom { event_type, data } => Self::Custom { event_type, data },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_envelope(kind: EventKind) -> EventEnvelope {
        EventEnvelope {
            event_id: EventId::from_string("EVT001"),
            session_id: SessionId::from_string("SESS001"),
            agent_id: AgentId::from_string("AGENT001"),
            branch_id: BranchId::from_string("main"),
            run_id: None,
            seq: 1,
            timestamp: 1_700_000_000_000_000,
            actor: EventActor::default(),
            schema: EventSchema::default(),
            parent_id: None,
            trace_id: None,
            span_id: None,
            digest: None,
            kind,
            metadata: HashMap::new(),
            schema_version: 1,
        }
    }

    #[test]
    fn error_raised_roundtrip() {
        let kind = EventKind::ErrorRaised {
            message: "boom".into(),
        };
        let json = serde_json::to_string(&kind).unwrap();
        assert!(json.contains("\"type\":\"ErrorRaised\""));
        let back: EventKind = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, EventKind::ErrorRaised { message } if message == "boom"));
    }

    #[test]
    fn heartbeat_roundtrip() {
        let kind = EventKind::Heartbeat {
            summary: "alive".into(),
            checkpoint_id: None,
        };
        let json = serde_json::to_string(&kind).unwrap();
        let back: EventKind = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, EventKind::Heartbeat { .. }));
    }

    #[test]
    fn state_estimated_roundtrip() {
        let kind = EventKind::StateEstimated {
            state: AgentStateVector::default(),
            mode: OperatingMode::Execute,
        };
        let json = serde_json::to_string(&kind).unwrap();
        let back: EventKind = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, EventKind::StateEstimated { .. }));
    }

    #[test]
    fn unknown_variant_becomes_custom() {
        let json = r#"{"type":"FutureFeature","key":"value","num":42}"#;
        let kind: EventKind = serde_json::from_str(json).unwrap();
        if let EventKind::Custom { event_type, data } = kind {
            assert_eq!(event_type, "FutureFeature");
            assert_eq!(data["key"], "value");
            assert_eq!(data["num"], 42);
        } else {
            panic!("should be Custom");
        }
    }

    #[test]
    fn full_envelope_roundtrip() {
        let envelope = make_envelope(EventKind::RunStarted {
            provider: "anthropic".into(),
            max_iterations: 10,
        });
        let json = serde_json::to_string(&envelope).unwrap();
        let back: EventEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(back.seq, 1);
        assert_eq!(back.schema_version, 1);
        assert!(matches!(back.kind, EventKind::RunStarted { .. }));
    }

    #[test]
    fn tool_call_lifecycle_roundtrip() {
        let requested = EventKind::ToolCallRequested {
            call_id: "c1".into(),
            tool_name: "read_file".into(),
            arguments: serde_json::json!({"path": "/etc/hosts"}),
            category: Some("fs".into()),
        };
        let json = serde_json::to_string(&requested).unwrap();
        let back: EventKind = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, EventKind::ToolCallRequested { .. }));
    }

    #[test]
    fn knowledge_events_roundtrip() {
        let searched = EventKind::KnowledgeSearched {
            query: "temporal validity".into(),
            result_count: 3,
            top_relevance: 0.82,
            duration_ms: 47,
        };
        let searched_json = serde_json::to_string(&searched).unwrap();
        let searched_back: EventKind = serde_json::from_str(&searched_json).unwrap();
        assert!(matches!(
            searched_back,
            EventKind::KnowledgeSearched {
                result_count: 3,
                duration_ms: 47,
                ..
            }
        ));

        let retrieved = EventKind::KnowledgeRetrieved {
            note_count: 8,
            context_tokens: 600,
            source: "wake_up".into(),
        };
        let retrieved_json = serde_json::to_string(&retrieved).unwrap();
        let retrieved_back: EventKind = serde_json::from_str(&retrieved_json).unwrap();
        assert!(matches!(
            retrieved_back,
            EventKind::KnowledgeRetrieved {
                note_count: 8,
                context_tokens: 600,
                ..
            }
        ));

        let evaluated = EventKind::KnowledgeEvaluated {
            health_score: 0.91,
            note_count: 64,
            contradictions: 1,
            missing_pages: 2,
            orphans: 3,
        };
        let evaluated_json = serde_json::to_string(&evaluated).unwrap();
        let evaluated_back: EventKind = serde_json::from_str(&evaluated_json).unwrap();
        assert!(matches!(
            evaluated_back,
            EventKind::KnowledgeEvaluated {
                note_count: 64,
                contradictions: 1,
                missing_pages: 2,
                orphans: 3,
                ..
            }
        ));
    }

    #[test]
    fn memory_events_roundtrip() {
        let proposed = EventKind::MemoryProposed {
            scope: MemoryScope::Agent,
            proposal_id: MemoryId::from_string("PROP001"),
            entries_ref: BlobHash::from_hex("abc"),
            source_run_id: None,
        };
        let json = serde_json::to_string(&proposed).unwrap();
        let back: EventKind = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, EventKind::MemoryProposed { .. }));
    }

    #[test]
    fn mode_changed_roundtrip() {
        let kind = EventKind::ModeChanged {
            from: OperatingMode::Execute,
            to: OperatingMode::Recover,
            reason: "error streak".into(),
        };
        let json = serde_json::to_string(&kind).unwrap();
        let back: EventKind = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, EventKind::ModeChanged { .. }));
    }

    #[test]
    fn schema_version_defaults_to_1() {
        let json = r#"{"event_id":"E1","session_id":"S1","branch_id":"main","seq":0,"timestamp":100,"kind":{"type":"ErrorRaised","message":"x"},"metadata":{}}"#;
        let envelope: EventEnvelope = serde_json::from_str(json).unwrap();
        assert_eq!(envelope.schema_version, 1);
    }

    #[test]
    fn hive_task_created_roundtrip() {
        let kind = EventKind::HiveTaskCreated {
            hive_task_id: HiveTaskId::from_string("HIVE001"),
            objective: "optimize scoring".into(),
            agent_count: 3,
        };
        let json = serde_json::to_string(&kind).unwrap();
        assert!(json.contains("\"type\":\"HiveTaskCreated\""));
        let back: EventKind = serde_json::from_str(&json).unwrap();
        assert!(matches!(
            back,
            EventKind::HiveTaskCreated { agent_count: 3, .. }
        ));
    }

    #[test]
    fn hive_artifact_shared_roundtrip() {
        let kind = EventKind::HiveArtifactShared {
            hive_task_id: HiveTaskId::from_string("HIVE001"),
            source_session_id: SessionId::from_string("SESS-A"),
            score: 0.87,
            mutation_summary: "rewrote parser".into(),
        };
        let json = serde_json::to_string(&kind).unwrap();
        let back: EventKind = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, EventKind::HiveArtifactShared { .. }));
    }

    #[test]
    fn hive_selection_made_roundtrip() {
        let kind = EventKind::HiveSelectionMade {
            hive_task_id: HiveTaskId::from_string("HIVE001"),
            winning_session_id: SessionId::from_string("SESS-B"),
            winning_score: 0.92,
            generation: 2,
        };
        let json = serde_json::to_string(&kind).unwrap();
        let back: EventKind = serde_json::from_str(&json).unwrap();
        assert!(matches!(
            back,
            EventKind::HiveSelectionMade { generation: 2, .. }
        ));
    }

    #[test]
    fn hive_generation_completed_roundtrip() {
        let kind = EventKind::HiveGenerationCompleted {
            hive_task_id: HiveTaskId::from_string("HIVE001"),
            generation: 3,
            best_score: 0.95,
            agent_results: serde_json::json!({"agents": 3, "improved": true}),
        };
        let json = serde_json::to_string(&kind).unwrap();
        let back: EventKind = serde_json::from_str(&json).unwrap();
        assert!(matches!(
            back,
            EventKind::HiveGenerationCompleted { generation: 3, .. }
        ));
    }

    #[test]
    fn hive_task_completed_roundtrip() {
        let kind = EventKind::HiveTaskCompleted {
            hive_task_id: HiveTaskId::from_string("HIVE001"),
            total_generations: 5,
            total_trials: 15,
            final_score: 0.98,
        };
        let json = serde_json::to_string(&kind).unwrap();
        let back: EventKind = serde_json::from_str(&json).unwrap();
        assert!(matches!(
            back,
            EventKind::HiveTaskCompleted {
                total_generations: 5,
                ..
            }
        ));
    }

    #[test]
    fn hive_full_envelope_roundtrip() {
        let envelope = make_envelope(EventKind::HiveTaskCreated {
            hive_task_id: HiveTaskId::from_string("HIVE-ENV"),
            objective: "test envelope".into(),
            agent_count: 5,
        });
        let json = serde_json::to_string(&envelope).unwrap();
        let back: EventEnvelope = serde_json::from_str(&json).unwrap();
        assert!(matches!(
            back.kind,
            EventKind::HiveTaskCreated { agent_count: 5, .. }
        ));
    }

    #[test]
    fn event_kind_variant_name() {
        assert_eq!(
            EventKind::PhaseEntered {
                phase: LoopPhase::Perceive
            }
            .variant_name(),
            "PhaseEntered"
        );
        assert_eq!(
            EventKind::TextDelta {
                delta: "x".into(),
                index: None
            }
            .variant_name(),
            "TextDelta"
        );
        assert_eq!(
            EventKind::RunFinished {
                reason: "done".into(),
                total_iterations: 1,
                final_answer: None,
                usage: None,
            }
            .variant_name(),
            "RunFinished"
        );
        assert_eq!(
            EventKind::UserMessage {
                content: "hi".into()
            }
            .variant_name(),
            "UserMessage"
        );
        assert_eq!(
            EventKind::ErrorRaised {
                message: "boom".into()
            }
            .variant_name(),
            "ErrorRaised"
        );
        assert_eq!(
            EventKind::KnowledgeSearched {
                query: "q".into(),
                result_count: 1,
                top_relevance: 0.5,
                duration_ms: 10,
            }
            .variant_name(),
            "KnowledgeSearched"
        );
        assert_eq!(
            EventKind::Custom {
                event_type: "Foo".into(),
                data: serde_json::json!(null),
            }
            .variant_name(),
            "Custom"
        );
        assert_eq!(
            EventKind::HiveTaskCreated {
                hive_task_id: HiveTaskId::from_string("H1"),
                objective: "test".into(),
                agent_count: 1,
            }
            .variant_name(),
            "HiveTaskCreated"
        );
    }

    #[test]
    fn voice_events_roundtrip() {
        let kind = EventKind::VoiceSessionStarted {
            voice_session_id: "vs1".into(),
            adapter: "openai-realtime".into(),
            model: "gpt-4o-realtime".into(),
            sample_rate_hz: 24000,
            channels: 1,
        };
        let json = serde_json::to_string(&kind).unwrap();
        let back: EventKind = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, EventKind::VoiceSessionStarted { .. }));
    }
}
