use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use aios_kernel::{AiosKernel, KernelBuilder};
use aios_model::{
    AgentStateVector, Capability, EventRecord, ModelRouting, OperatingMode, PolicySet, SessionId,
    SessionManifest, ToolCall,
};
use anyhow::Result;
use async_stream::stream;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use clap::Parser;
use futures_util::Stream;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::info;
use uuid::Uuid;

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
    proposed_tool: Option<ProposedToolRequest>,
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
    from_sequence: Option<u64>,
    limit: Option<usize>,
}

#[derive(Debug, Serialize)]
struct EventListResponse {
    session_id: SessionId,
    from_sequence: u64,
    events: Vec<EventRecord>,
}

#[derive(Debug, Deserialize, Default)]
struct EventStreamQuery {
    cursor: Option<u64>,
    replay_limit: Option<usize>,
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

    let state = AppState { kernel };

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/sessions", post(create_session))
        .route("/sessions/{session_id}/ticks", post(tick_session))
        .route("/sessions/{session_id}/events", get(list_events))
        .route("/sessions/{session_id}/events/stream", get(stream_events))
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

    let result = state
        .kernel
        .tick(
            session_id,
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

async fn list_events(
    Path(session_id): Path<String>,
    Query(query): Query<EventListQuery>,
    State(state): State<AppState>,
) -> ApiResult<Json<EventListResponse>> {
    let session_id = parse_session_id(&session_id)?;
    let from_sequence = query.from_sequence.unwrap_or(1).max(1);
    let limit = query.limit.unwrap_or(200).clamp(1, 5000);

    let events = state
        .kernel
        .read_events(session_id, from_sequence, limit)
        .await
        .map_err(ApiError::internal)?;

    Ok(Json(EventListResponse {
        session_id,
        from_sequence,
        events,
    }))
}

async fn stream_events(
    Path(session_id): Path<String>,
    Query(query): Query<EventStreamQuery>,
    State(state): State<AppState>,
) -> ApiResult<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    let session_id = parse_session_id(&session_id)?;
    let mut next_sequence = query.cursor.map_or(1, |cursor| cursor.saturating_add(1));
    let replay_limit = query.replay_limit.unwrap_or(500).clamp(1, 5000);

    let replay_events = state
        .kernel
        .read_events(session_id, next_sequence, replay_limit)
        .await
        .map_err(ApiError::internal)?;

    if let Some(last_event) = replay_events.last() {
        next_sequence = last_event.sequence.saturating_add(1);
    }

    let mut subscription = state.kernel.subscribe_events();
    let stream = stream! {
        for event in replay_events {
            yield Ok(as_sse_event("kernel.event", &event));
        }

        let mut expected_sequence = next_sequence;
        loop {
            match subscription.recv().await {
                Ok(event) => {
                    if event.session_id != session_id || event.sequence < expected_sequence {
                        continue;
                    }
                    expected_sequence = event.sequence.saturating_add(1);
                    yield Ok(as_sse_event("kernel.event", &event));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    let lag_payload = json!({ "skipped": skipped }).to_string();
                    yield Ok(Event::default().event("stream.lagged").data(lag_payload));
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
    use super::parse_session_id;

    #[test]
    fn parse_session_id_rejects_invalid_uuid() {
        let result = parse_session_id("not-a-uuid");
        assert!(result.is_err());
    }
}
