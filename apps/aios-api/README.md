# aios-api

HTTP control-plane for `aiOS`.

## Purpose

Expose session lifecycle, ticking, approvals, and event replay/streaming over HTTP/SSE.

## Endpoints

- `GET /healthz`
- `POST /sessions`
- `POST /sessions/{session_id}/ticks`
- `POST /sessions/{session_id}/approvals/{approval_id}`
- `GET /sessions/{session_id}/events`
- `GET /sessions/{session_id}/events/stream?cursor=...`

## Run

```bash
cargo run -p aios-api -- --root .aios --listen 127.0.0.1:8787
```

## Dependencies

- `aios-kernel`
- `aios-model`
- `axum`, `tower-http`, `tokio`, `tracing`
