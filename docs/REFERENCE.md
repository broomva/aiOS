# Reference

## Quality Gate

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
scripts/validate_openapi_live.sh
```

## Runtime Apps

```bash
cargo run -p aiosd -- --root .aios
cargo run -p aios-api -- --root .aios --listen 127.0.0.1:8787
```

## API Endpoints

- `GET /healthz`
- `GET /openapi.json`
- `GET /docs`
- `POST /sessions`
- `POST /sessions/{session_id}/ticks`
- `POST /sessions/{session_id}/approvals/{approval_id}`
- `GET /sessions/{session_id}/events?from_sequence=1&limit=200`
- `GET /sessions/{session_id}/events/stream?cursor=0&replay_limit=500`
- `GET /sessions/{session_id}/events/stream/vercel-ai-sdk-v6?cursor=0&replay_limit=500`
- `POST /sessions/{session_id}/voice/start`
- `GET /sessions/{session_id}/voice/stream?voice_session_id=...` (WebSocket)

### Vercel AI SDK v6 Stream Contract

- Response header: `x-vercel-ai-ui-message-stream: v1`
- Payload framing: SSE `data: {json}` parts + terminal `data: [DONE]` on stream close.
- Event bridge: each kernel event is surfaced as `data-aios-event` with stable sequence id.

## Key Workspace Paths

- Session root: `<root>/sessions/<session-id>/`
- Event log: `<root>/kernel/events/<session-id>.jsonl`
- Checkpoints: `<root>/sessions/<session-id>/checkpoints/`
- Tool reports: `<root>/sessions/<session-id>/tools/runs/`
