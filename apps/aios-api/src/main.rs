use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use aios_kernel::{AiosKernel, KernelBuilder};
use aios_model::{
    AgentStateVector, BranchId, BranchInfo, BranchMergeResult, Capability, EventKind, EventRecord,
    ModelRouting, OperatingMode, PolicySet, SessionId, SessionManifest, ToolCall,
};
use anyhow::Result;
use async_stream::stream;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::header::CACHE_CONTROL;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::Html;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use clap::Parser;
use futures_util::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::info;
use uuid::Uuid;

mod openapi;
mod vercel_v6;
mod voice;

use crate::openapi::{openapi_spec, scalar_docs_html};
use crate::vercel_v6::{
    VERCEL_AI_SDK_V6_STREAM_HEADER, VERCEL_AI_SDK_V6_STREAM_VERSION, kernel_event_parts,
    part_as_sse_event,
};
use crate::voice::{PersonaplexProcessContract, StubPersonaplexAdapter, VoiceSessionConfig};

#[derive(Debug, Parser)]
#[command(name = "aios-api")]
#[command(about = "aiOS control-plane API")]
struct Cli {
    #[arg(long, default_value = ".aios")]
    root: PathBuf,
    #[arg(long, default_value = "127.0.0.1:8787")]
    listen: SocketAddr,
}

#[derive(Clone)]
struct AppState {
    kernel: AiosKernel,
    voice_adapter: StubPersonaplexAdapter,
    voice_sessions: Arc<RwLock<HashMap<Uuid, ActiveVoiceSession>>>,
}

#[derive(Debug, Clone)]
struct ActiveVoiceSession {
    session_id: SessionId,
    format: String,
}

#[derive(Debug, Deserialize, Default)]
struct CreateSessionRequest {
    owner: Option<String>,
    policy: Option<PolicySet>,
    model_routing: Option<ModelRouting>,
}

#[derive(Debug, Deserialize)]
struct TickRequest {
    objective: String,
    branch: Option<String>,
    proposed_tool: Option<ProposedToolRequest>,
}

#[derive(Debug, Deserialize)]
struct CreateBranchRequest {
    branch: String,
    from_branch: Option<String>,
    fork_sequence: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct ProposedToolRequest {
    tool_name: String,
    input: serde_json::Value,
    #[serde(default)]
    requested_capabilities: Vec<Capability>,
}

#[derive(Debug, Serialize)]
struct TickResponse {
    session_id: SessionId,
    mode: OperatingMode,
    state: AgentStateVector,
    events_emitted: u64,
    last_sequence: u64,
}

#[derive(Debug, Deserialize)]
struct ResolveApprovalRequest {
    approved: bool,
    actor: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct EventListQuery {
    branch: Option<String>,
    from_sequence: Option<u64>,
    limit: Option<usize>,
}

#[derive(Debug, Serialize)]
struct EventListResponse {
    session_id: SessionId,
    branch: BranchId,
    from_sequence: u64,
    events: Vec<EventRecord>,
}

#[derive(Debug, Serialize)]
struct BranchListResponse {
    session_id: SessionId,
    branches: Vec<BranchInfo>,
}

#[derive(Debug, Deserialize, Default)]
struct MergeBranchRequest {
    target_branch: Option<String>,
}

#[derive(Debug, Serialize)]
struct BranchMergeResponse {
    session_id: SessionId,
    result: BranchMergeResult,
}

#[derive(Debug, Deserialize, Default)]
struct EventStreamQuery {
    branch: Option<String>,
    cursor: Option<u64>,
    replay_limit: Option<usize>,
}

#[derive(Debug, Deserialize, Default)]
struct VoiceStartRequest {
    role_prompt: Option<String>,
    voice_prompt_ref: Option<String>,
    sample_rate_hz: Option<u32>,
    channels: Option<u8>,
    format: Option<String>,
}

#[derive(Debug, Serialize)]
struct VoiceStartResponse {
    session_id: SessionId,
    voice_session_id: Uuid,
    model: String,
    sample_rate_hz: u32,
    channels: u8,
    format: String,
    ws_path: String,
}

#[derive(Debug, Deserialize)]
struct VoiceStreamQuery {
    voice_session_id: String,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn internal(error: impl std::fmt::Display) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: error.to_string(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "error": self.message }))).into_response()
    }
}

type ApiResult<T> = Result<T, ApiError>;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .json()
        .init();

    let cli = Cli::parse();
    let kernel = KernelBuilder::new(&cli.root).build();
    let voice_adapter = StubPersonaplexAdapter::new(PersonaplexProcessContract::default());

    let state = AppState {
        kernel,
        voice_adapter,
        voice_sessions: Arc::new(RwLock::new(HashMap::new())),
    };

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/openapi.json", get(openapi_json))
        .route("/docs", get(docs))
        .route("/docs/", get(docs))
        .route("/sessions", post(create_session))
        .route("/sessions/{session_id}/ticks", post(tick_session))
        .route(
            "/sessions/{session_id}/branches",
            post(create_branch).get(list_branches),
        )
        .route(
            "/sessions/{session_id}/branches/{branch_id}/merge",
            post(merge_branch),
        )
        .route("/sessions/{session_id}/events", get(list_events))
        .route("/sessions/{session_id}/events/stream", get(stream_events))
        .route(
            "/sessions/{session_id}/events/stream/vercel-ai-sdk-v6",
            get(stream_events_vercel_ai_sdk_v6),
        )
        .route(
            "/sessions/{session_id}/voice/start",
            post(start_voice_session),
        )
        .route("/sessions/{session_id}/voice/stream", get(stream_voice_ws))
        .route(
            "/sessions/{session_id}/approvals/{approval_id}",
            post(resolve_approval),
        )
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(cli.listen).await?;
    info!(listen = %cli.listen, root = %cli.root.display(), "aios-api listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn healthz() -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "service": "aios-api"
    }))
}

async fn openapi_json() -> Json<serde_json::Value> {
    Json(openapi_spec())
}

async fn docs() -> Html<String> {
    Html(scalar_docs_html("/openapi.json"))
}

async fn create_session(
    State(state): State<AppState>,
    Json(request): Json<CreateSessionRequest>,
) -> ApiResult<Json<SessionManifest>> {
    let owner = request.owner.unwrap_or_else(|| "api".to_owned());
    let policy = request.policy.unwrap_or_default();

    let manifest = state
        .kernel
        .create_session(owner, policy, request.model_routing)
        .await
        .map_err(ApiError::internal)?;

    Ok(Json(manifest))
}

async fn tick_session(
    Path(session_id): Path<String>,
    State(state): State<AppState>,
    Json(request): Json<TickRequest>,
) -> ApiResult<Json<TickResponse>> {
    let session_id = parse_session_id(&session_id)?;
    let branch_id = parse_branch_id(request.branch.as_deref())?;

    let result = state
        .kernel
        .tick_on_branch(
            session_id,
            branch_id,
            request.objective,
            request.proposed_tool.map(|proposed_tool| {
                ToolCall::new(
                    proposed_tool.tool_name,
                    proposed_tool.input,
                    proposed_tool.requested_capabilities,
                )
            }),
        )
        .await
        .map_err(ApiError::internal)?;

    Ok(Json(TickResponse {
        session_id: result.session_id,
        mode: result.mode,
        state: result.state,
        events_emitted: result.events_emitted,
        last_sequence: result.last_sequence,
    }))
}

async fn create_branch(
    Path(session_id): Path<String>,
    State(state): State<AppState>,
    Json(request): Json<CreateBranchRequest>,
) -> ApiResult<Json<BranchInfo>> {
    let session_id = parse_session_id(&session_id)?;
    let branch_id = parse_branch_id(Some(&request.branch))?;
    let from_branch = match request.from_branch.as_deref() {
        Some(raw) => Some(parse_branch_id(Some(raw))?),
        None => None,
    };

    let branch = state
        .kernel
        .create_branch(session_id, branch_id, from_branch, request.fork_sequence)
        .await
        .map_err(ApiError::internal)?;

    Ok(Json(branch))
}

async fn list_branches(
    Path(session_id): Path<String>,
    State(state): State<AppState>,
) -> ApiResult<Json<BranchListResponse>> {
    let session_id = parse_session_id(&session_id)?;
    let branches = state
        .kernel
        .list_branches(session_id)
        .await
        .map_err(ApiError::internal)?;

    Ok(Json(BranchListResponse {
        session_id,
        branches,
    }))
}

async fn merge_branch(
    Path((session_id, branch_id)): Path<(String, String)>,
    State(state): State<AppState>,
    Json(request): Json<MergeBranchRequest>,
) -> ApiResult<Json<BranchMergeResponse>> {
    let session_id = parse_session_id(&session_id)?;
    let source_branch = parse_branch_id(Some(&branch_id))?;
    let target_branch = parse_branch_id(request.target_branch.as_deref())?;

    let result = state
        .kernel
        .merge_branch(session_id, source_branch, target_branch)
        .await
        .map_err(ApiError::internal)?;

    Ok(Json(BranchMergeResponse { session_id, result }))
}

async fn list_events(
    Path(session_id): Path<String>,
    Query(query): Query<EventListQuery>,
    State(state): State<AppState>,
) -> ApiResult<Json<EventListResponse>> {
    let session_id = parse_session_id(&session_id)?;
    let branch_id = parse_branch_id(query.branch.as_deref())?;
    let from_sequence = query.from_sequence.unwrap_or(1).max(1);
    let limit = query.limit.unwrap_or(200).clamp(1, 5000);

    let events = state
        .kernel
        .read_events_on_branch(session_id, branch_id.clone(), from_sequence, limit)
        .await
        .map_err(ApiError::internal)?;

    Ok(Json(EventListResponse {
        session_id,
        branch: branch_id,
        from_sequence,
        events,
    }))
}

fn replay_start_sequence(cursor: Option<u64>) -> u64 {
    cursor.map_or(1, |value| value.saturating_add(1))
}

fn replay_window_limit(replay_limit: Option<usize>) -> usize {
    replay_limit.unwrap_or(500).clamp(1, 5000)
}

async fn load_replay_events(
    kernel: &AiosKernel,
    session_id: SessionId,
    branch_id: &BranchId,
    from_sequence: u64,
    limit: usize,
) -> ApiResult<Vec<EventRecord>> {
    kernel
        .read_events_on_branch(session_id, branch_id.clone(), from_sequence.max(1), limit)
        .await
        .map_err(ApiError::internal)
}

async fn load_gap_events(
    kernel: &AiosKernel,
    session_id: SessionId,
    branch_id: &BranchId,
    from_sequence: u64,
    through_sequence: u64,
    limit: usize,
) -> ApiResult<Vec<EventRecord>> {
    if from_sequence > through_sequence {
        return Ok(Vec::new());
    }
    let mut events =
        load_replay_events(kernel, session_id, branch_id, from_sequence, limit).await?;
    events.retain(|event| event.sequence <= through_sequence);
    Ok(events)
}

async fn stream_events(
    Path(session_id): Path<String>,
    Query(query): Query<EventStreamQuery>,
    State(state): State<AppState>,
) -> ApiResult<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    let session_id = parse_session_id(&session_id)?;
    let branch_id = parse_branch_id(query.branch.as_deref())?;
    let mut next_sequence = replay_start_sequence(query.cursor);
    let replay_limit = replay_window_limit(query.replay_limit);
    let kernel = state.kernel.clone();

    let replay_events =
        load_replay_events(&kernel, session_id, &branch_id, next_sequence, replay_limit).await?;

    if let Some(last_event) = replay_events.last() {
        next_sequence = last_event.sequence.saturating_add(1);
    }

    let mut subscription = kernel.subscribe_events();
    let stream = stream! {
        for event in replay_events {
            yield Ok(as_sse_event("kernel.event", &event));
        }

        let mut expected_sequence = next_sequence;
        loop {
            match subscription.recv().await {
                Ok(event) => {
                    if event.session_id != session_id
                        || event.branch_id != branch_id
                        || event.sequence < expected_sequence
                    {
                        continue;
                    }

                    if event.sequence > expected_sequence {
                        match load_gap_events(
                            &kernel,
                            session_id,
                            &branch_id,
                            expected_sequence,
                            event.sequence,
                            replay_limit,
                        )
                        .await
                        {
                            Ok(backfill_events) => {
                                let mut replayed_current = false;
                                for backfill_event in backfill_events {
                                    if backfill_event.sequence < expected_sequence {
                                        continue;
                                    }
                                    if backfill_event.sequence == event.sequence {
                                        replayed_current = true;
                                    }
                                    expected_sequence = backfill_event.sequence.saturating_add(1);
                                    yield Ok(as_sse_event("kernel.event", &backfill_event));
                                }
                                if replayed_current {
                                    continue;
                                }
                            }
                            Err(error) => {
                                let payload = json!({
                                    "error": error.message,
                                    "from_sequence": expected_sequence,
                                    "through_sequence": event.sequence,
                                })
                                .to_string();
                                yield Ok(Event::default().event("stream.error").data(payload));
                                continue;
                            }
                        }
                    }

                    if event.sequence != expected_sequence {
                        continue;
                    }

                    expected_sequence = event.sequence.saturating_add(1);
                    yield Ok(as_sse_event("kernel.event", &event));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    let lag_payload = json!({
                        "skipped": skipped,
                        "from_sequence": expected_sequence,
                    })
                    .to_string();
                    yield Ok(Event::default().event("stream.lagged").data(lag_payload));
                    match load_replay_events(
                        &kernel,
                        session_id,
                        &branch_id,
                        expected_sequence,
                        replay_limit,
                    )
                    .await
                    {
                        Ok(backfill_events) => {
                            for backfill_event in backfill_events {
                                if backfill_event.sequence < expected_sequence {
                                    continue;
                                }
                                expected_sequence = backfill_event.sequence.saturating_add(1);
                                yield Ok(as_sse_event("kernel.event", &backfill_event));
                            }
                        }
                        Err(error) => {
                            let payload = json!({
                                "error": error.message,
                                "from_sequence": expected_sequence,
                            })
                            .to_string();
                            yield Ok(Event::default().event("stream.error").data(payload));
                        }
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
}

async fn stream_events_vercel_ai_sdk_v6(
    Path(session_id): Path<String>,
    Query(query): Query<EventStreamQuery>,
    State(state): State<AppState>,
) -> ApiResult<(
    HeaderMap,
    Sse<impl Stream<Item = Result<Event, Infallible>>>,
)> {
    let session_id = parse_session_id(&session_id)?;
    let branch_id = parse_branch_id(query.branch.as_deref())?;
    let mut next_sequence = replay_start_sequence(query.cursor);
    let replay_limit = replay_window_limit(query.replay_limit);
    let kernel = state.kernel.clone();

    let replay_events =
        load_replay_events(&kernel, session_id, &branch_id, next_sequence, replay_limit).await?;

    if let Some(last_event) = replay_events.last() {
        next_sequence = last_event.sequence.saturating_add(1);
    }

    let mut subscription = kernel.subscribe_events();
    let stream = stream! {
        for event in replay_events {
            for part in kernel_event_parts(&event) {
                yield Ok(part_as_sse_event(&part));
            }
        }

        let mut expected_sequence = next_sequence;
        loop {
            match subscription.recv().await {
                Ok(event) => {
                    if event.session_id != session_id
                        || event.branch_id != branch_id
                        || event.sequence < expected_sequence
                    {
                        continue;
                    }

                    if event.sequence > expected_sequence {
                        match load_gap_events(
                            &kernel,
                            session_id,
                            &branch_id,
                            expected_sequence,
                            event.sequence,
                            replay_limit,
                        )
                        .await
                        {
                            Ok(backfill_events) => {
                                let mut replayed_current = false;
                                for backfill_event in backfill_events {
                                    if backfill_event.sequence < expected_sequence {
                                        continue;
                                    }
                                    if backfill_event.sequence == event.sequence {
                                        replayed_current = true;
                                    }
                                    expected_sequence = backfill_event.sequence.saturating_add(1);
                                    for part in kernel_event_parts(&backfill_event) {
                                        yield Ok(part_as_sse_event(&part));
                                    }
                                }
                                if replayed_current {
                                    continue;
                                }
                            }
                            Err(error) => {
                                let payload = json!({
                                    "type": "data-aios-stream-status",
                                    "data": {
                                        "status": "error",
                                        "message": error.message,
                                        "from_sequence": expected_sequence,
                                        "through_sequence": event.sequence,
                                    },
                                })
                                .to_string();
                                yield Ok(Event::default().data(payload));
                                continue;
                            }
                        }
                    }

                    if event.sequence != expected_sequence {
                        continue;
                    }

                    expected_sequence = event.sequence.saturating_add(1);
                    for part in kernel_event_parts(&event) {
                        yield Ok(part_as_sse_event(&part));
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    let lag_payload = json!({
                        "type": "data-aios-stream-status",
                        "data": {
                            "status": "lagged",
                            "skipped": skipped,
                            "from_sequence": expected_sequence,
                        },
                    })
                    .to_string();
                    yield Ok(Event::default().data(lag_payload));
                    match load_replay_events(
                        &kernel,
                        session_id,
                        &branch_id,
                        expected_sequence,
                        replay_limit,
                    )
                    .await
                    {
                        Ok(backfill_events) => {
                            for backfill_event in backfill_events {
                                if backfill_event.sequence < expected_sequence {
                                    continue;
                                }
                                expected_sequence = backfill_event.sequence.saturating_add(1);
                                for part in kernel_event_parts(&backfill_event) {
                                    yield Ok(part_as_sse_event(&part));
                                }
                            }
                        }
                        Err(error) => {
                            let payload = json!({
                                "type": "data-aios-stream-status",
                                "data": {
                                    "status": "error",
                                    "message": error.message,
                                    "from_sequence": expected_sequence,
                                },
                            })
                            .to_string();
                            yield Ok(Event::default().data(payload));
                        }
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    yield Ok(Event::default().data("[DONE]"));
                    break;
                }
            }
        }
    };

    let sse = Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    );

    let mut headers = HeaderMap::new();
    headers.insert(
        VERCEL_AI_SDK_V6_STREAM_HEADER,
        HeaderValue::from_static(VERCEL_AI_SDK_V6_STREAM_VERSION),
    );
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));

    Ok((headers, sse))
}

async fn start_voice_session(
    Path(session_id): Path<String>,
    State(state): State<AppState>,
    Json(request): Json<VoiceStartRequest>,
) -> ApiResult<Json<VoiceStartResponse>> {
    let session_id = parse_session_id(&session_id)?;
    let sample_rate_hz = request.sample_rate_hz.unwrap_or(24_000);
    let channels = request.channels.unwrap_or(1);

    if channels == 0 {
        return Err(ApiError::bad_request("channels must be >= 1"));
    }

    let format = request
        .format
        .unwrap_or_else(|| format!("audio/pcm;rate={sample_rate_hz}"));

    let config = VoiceSessionConfig {
        role_prompt: request.role_prompt,
        voice_prompt_ref: request.voice_prompt_ref,
        sample_rate_hz,
        channels,
        format: format.clone(),
    };

    let voice_session_id = state
        .voice_adapter
        .start_session(session_id.0, &config)
        .await
        .map_err(ApiError::internal)?;

    let contract = state.voice_adapter.contract().clone();

    state.voice_sessions.write().await.insert(
        voice_session_id,
        ActiveVoiceSession {
            session_id,
            format: format.clone(),
        },
    );

    state
        .kernel
        .record_external_event(
            session_id,
            EventKind::VoiceSessionStarted {
                voice_session_id,
                adapter: contract.command,
                model: contract.model_id.clone(),
                sample_rate_hz,
                channels,
            },
        )
        .await
        .map_err(ApiError::internal)?;

    Ok(Json(VoiceStartResponse {
        session_id,
        voice_session_id,
        model: contract.model_id,
        sample_rate_hz,
        channels,
        format,
        ws_path: format!(
            "/sessions/{}/voice/stream?voice_session_id={voice_session_id}",
            session_id.0
        ),
    }))
}

async fn stream_voice_ws(
    Path(session_id): Path<String>,
    Query(query): Query<VoiceStreamQuery>,
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> ApiResult<Response> {
    let session_id = parse_session_id(&session_id)?;
    let voice_session_id = Uuid::parse_str(&query.voice_session_id)
        .map_err(|error| ApiError::bad_request(format!("invalid voice session id: {error}")))?;

    let voice_state = {
        let guard = state.voice_sessions.read().await;
        guard.get(&voice_session_id).cloned().ok_or_else(|| {
            ApiError::bad_request(format!("voice session not found: {voice_session_id}"))
        })?
    };

    if voice_state.session_id != session_id {
        return Err(ApiError::bad_request(
            "voice session does not belong to requested session",
        ));
    }

    Ok(ws.on_upgrade(move |socket| {
        handle_voice_socket(state, voice_state, voice_session_id, socket)
    }))
}

async fn handle_voice_socket(
    state: AppState,
    voice_state: ActiveVoiceSession,
    voice_session_id: Uuid,
    mut socket: WebSocket,
) {
    let mut input_chunks = 0_u64;
    let mut output_chunks = 0_u64;

    loop {
        let next = socket.next().await;
        let Some(message) = next else {
            break;
        };

        match message {
            Ok(Message::Binary(audio_chunk)) => {
                input_chunks += 1;
                let _ = state
                    .kernel
                    .record_external_event(
                        voice_state.session_id,
                        EventKind::VoiceInputChunk {
                            voice_session_id,
                            chunk_index: input_chunks,
                            bytes: audio_chunk.len(),
                            format: voice_state.format.clone(),
                        },
                    )
                    .await;

                match state
                    .voice_adapter
                    .process_audio_chunk(voice_session_id, &audio_chunk)
                    .await
                {
                    Ok(output_chunk) => {
                        output_chunks += 1;
                        let _ = state
                            .kernel
                            .record_external_event(
                                voice_state.session_id,
                                EventKind::VoiceOutputChunk {
                                    voice_session_id,
                                    chunk_index: output_chunks,
                                    bytes: output_chunk.len(),
                                    format: voice_state.format.clone(),
                                },
                            )
                            .await;

                        if socket
                            .send(Message::Binary(output_chunk.into()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(error) => {
                        let _ = state
                            .kernel
                            .record_external_event(
                                voice_state.session_id,
                                EventKind::VoiceAdapterError {
                                    voice_session_id,
                                    message: error.to_string(),
                                },
                            )
                            .await;
                        break;
                    }
                }
            }
            Ok(Message::Text(text)) => {
                if text.trim().eq_ignore_ascii_case("stop") {
                    break;
                }
            }
            Ok(Message::Close(_)) => break,
            Ok(Message::Ping(payload)) => {
                if socket.send(Message::Pong(payload)).await.is_err() {
                    break;
                }
            }
            Ok(Message::Pong(_)) => {}
            Err(error) => {
                let _ = state
                    .kernel
                    .record_external_event(
                        voice_state.session_id,
                        EventKind::VoiceAdapterError {
                            voice_session_id,
                            message: error.to_string(),
                        },
                    )
                    .await;
                break;
            }
        }
    }

    let _ = state.voice_adapter.stop_session(voice_session_id).await;
    let _ = state
        .kernel
        .record_external_event(
            voice_state.session_id,
            EventKind::VoiceSessionStopped {
                voice_session_id,
                reason: "websocket disconnected".to_owned(),
            },
        )
        .await;
    state.voice_sessions.write().await.remove(&voice_session_id);
}

async fn resolve_approval(
    Path((session_id, approval_id)): Path<(String, String)>,
    State(state): State<AppState>,
    Json(request): Json<ResolveApprovalRequest>,
) -> ApiResult<StatusCode> {
    let session_id = parse_session_id(&session_id)?;
    let approval_id = Uuid::parse_str(&approval_id)
        .map_err(|error| ApiError::bad_request(format!("invalid approval id: {error}")))?;
    let actor = request.actor.unwrap_or_else(|| "api".to_owned());

    state
        .kernel
        .resolve_approval(session_id, approval_id, request.approved, actor)
        .await
        .map_err(ApiError::internal)?;

    Ok(StatusCode::NO_CONTENT)
}

fn parse_session_id(raw: &str) -> ApiResult<SessionId> {
    let uuid = Uuid::parse_str(raw)
        .map_err(|error| ApiError::bad_request(format!("invalid session id: {error}")))?;
    Ok(SessionId(uuid))
}

fn parse_branch_id(raw: Option<&str>) -> ApiResult<BranchId> {
    let value = raw.unwrap_or("main").trim();
    if value.is_empty() {
        return Err(ApiError::bad_request("branch must not be empty"));
    }
    Ok(BranchId::new(value))
}

fn as_sse_event(event_name: &str, event: &EventRecord) -> Event {
    let payload = serde_json::to_string(event)
        .unwrap_or_else(|error| json!({ "error": error.to_string() }).to_string());
    Event::default()
        .id(event.sequence.to_string())
        .event(event_name)
        .data(payload)
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(%error, "failed to install Ctrl+C handler");
        }
    };

    #[cfg(unix)]
    {
        let terminate = async {
            match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
                Ok(mut signal) => {
                    signal.recv().await;
                }
                Err(error) => {
                    tracing::error!(%error, "failed to install SIGTERM handler");
                }
            }
        };

        tokio::select! {
            _ = ctrl_c => {},
            _ = terminate => {},
        }
    }

    #[cfg(not(unix))]
    {
        ctrl_c.await;
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::{Path as StdPath, PathBuf};
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    use aios_kernel::KernelBuilder;
    use aios_model::{BranchId, PolicySet};
    use axum::Json;
    use axum::extract::{Path, State};
    use axum::http::StatusCode;
    use tokio::fs;
    use tokio::sync::RwLock;

    use super::{
        AppState, CreateBranchRequest, MergeBranchRequest, PersonaplexProcessContract,
        StubPersonaplexAdapter, create_branch, list_branches, merge_branch, parse_branch_id,
        parse_session_id, replay_start_sequence, replay_window_limit,
    };

    fn unique_test_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("{name}-{nanos}"))
    }

    fn test_state(root: &StdPath) -> AppState {
        AppState {
            kernel: KernelBuilder::new(root).build(),
            voice_adapter: StubPersonaplexAdapter::new(PersonaplexProcessContract::default()),
            voice_sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    #[test]
    fn parse_session_id_rejects_invalid_uuid() {
        let result = parse_session_id("not-a-uuid");
        assert!(result.is_err());
    }

    #[test]
    fn parse_branch_id_defaults_to_main() {
        let branch = parse_branch_id(None).expect("default branch");
        assert_eq!(branch.as_str(), "main");
    }

    #[test]
    fn parse_branch_id_rejects_empty_string() {
        let result = parse_branch_id(Some(" "));
        assert!(result.is_err());
    }

    #[test]
    fn replay_start_sequence_uses_next_after_cursor() {
        assert_eq!(replay_start_sequence(None), 1);
        assert_eq!(replay_start_sequence(Some(0)), 1);
        assert_eq!(replay_start_sequence(Some(41)), 42);
    }

    #[test]
    fn replay_window_limit_is_bounded() {
        assert_eq!(replay_window_limit(None), 500);
        assert_eq!(replay_window_limit(Some(0)), 1);
        assert_eq!(replay_window_limit(Some(12)), 12);
        assert_eq!(replay_window_limit(Some(20_000)), 5_000);
    }

    #[tokio::test]
    async fn branch_handlers_create_list_merge_round_trip() {
        let root = unique_test_root("aios-api-branches");
        let state = test_state(&root);

        let session = state
            .kernel
            .create_session("api-test", PolicySet::default(), None)
            .await
            .expect("create session");
        let session_id = session.session_id.0.to_string();
        let feature = "feature-api".to_owned();

        let Json(created) = create_branch(
            Path(session_id.clone()),
            State(state.clone()),
            Json(CreateBranchRequest {
                branch: feature.clone(),
                from_branch: Some("main".to_owned()),
                fork_sequence: None,
            }),
        )
        .await
        .expect("create branch");
        assert_eq!(created.branch_id.as_str(), feature.as_str());

        let Json(listing) = list_branches(Path(session_id.clone()), State(state.clone()))
            .await
            .expect("list branches");
        assert_eq!(listing.session_id, session.session_id);
        assert!(
            listing
                .branches
                .iter()
                .any(|branch| branch.branch_id.as_str() == feature.as_str())
        );

        let Json(merge) = merge_branch(
            Path((session_id, feature.clone())),
            State(state.clone()),
            Json(MergeBranchRequest {
                target_branch: Some("main".to_owned()),
            }),
        )
        .await
        .expect("merge branch");
        assert_eq!(merge.result.source_branch.as_str(), feature.as_str());
        assert_eq!(merge.result.target_branch.as_str(), "main");

        let tick_error = state
            .kernel
            .tick_on_branch(
                session.session_id,
                BranchId::new(feature.clone()),
                "post-merge tick should fail",
                None,
            )
            .await
            .expect_err("merged branch should be read-only");
        assert!(tick_error.to_string().contains("read-only"));

        let _ = fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn create_branch_handler_returns_error_for_invalid_fork_sequence() {
        let root = unique_test_root("aios-api-branch-errors");
        let state = test_state(&root);

        let session = state
            .kernel
            .create_session("api-test", PolicySet::default(), None)
            .await
            .expect("create session");

        let result = create_branch(
            Path(session.session_id.0.to_string()),
            State(state.clone()),
            Json(CreateBranchRequest {
                branch: "feature-invalid".to_owned(),
                from_branch: Some("main".to_owned()),
                fork_sequence: Some(1_000),
            }),
        )
        .await;

        let error = result.expect_err("fork sequence beyond head should fail");
        assert_eq!(error.status, StatusCode::INTERNAL_SERVER_ERROR);
        assert!(error.message.contains("exceeds source branch head"));

        let _ = fs::remove_dir_all(root).await;
    }
}
