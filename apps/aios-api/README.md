# aios-api

HTTP control-plane for `aiOS`.

## Purpose

Expose session lifecycle, ticking, approvals, and event replay/streaming over HTTP/SSE.

## Endpoints

- `GET /healthz`
- `GET /openapi.json`
- `GET /docs` (Scalar interactive docs)
- `POST /sessions`
- `POST /sessions/{session_id}/ticks`
- `POST /sessions/{session_id}/approvals/{approval_id}`
- `GET /sessions/{session_id}/events`
- `GET /sessions/{session_id}/events/stream?cursor=...`
- `GET /sessions/{session_id}/events/stream/vercel-ai-sdk-v6?cursor=...`
- `POST /sessions/{session_id}/voice/start`
- `GET /sessions/{session_id}/voice/stream?voice_session_id=...` (WebSocket)

## Run

```bash
cargo run -p aios-api -- --root .aios --listen 127.0.0.1:8787
```

## Dependencies

- `aios-kernel`
- `aios-model`
- `axum`, `tower-http`, `tokio`, `tracing`

## Voice Adapter Contract (Stub)

The initial PersonaPlex integration is a stub process contract in `src/voice.rs`:
- `PersonaplexProcessContract` defines command/env/protocol metadata.
- `StubPersonaplexAdapter` provides loopback audio behavior and lifecycle checks.

## Vercel AI SDK v6 Interface

`GET /sessions/{session_id}/events/stream/vercel-ai-sdk-v6` emits Server-Sent Events in the
Vercel AI SDK v6 UIMessage stream format and sets:

- `x-vercel-ai-ui-message-stream: v1`

Kernel events are emitted as custom `data-aios-event` parts, wrapped with `start`,
`start-step`, `finish-step`, and `finish` parts.

## OpenAPI + Scalar

- OpenAPI JSON: `GET /openapi.json`
- Interactive docs: `GET /docs` (served with Scalar API Reference)
