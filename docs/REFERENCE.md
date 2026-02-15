# Reference

## Quality Gate

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Runtime Apps

```bash
cargo run -p aiosd -- --root .aios
cargo run -p aios-api -- --root .aios --listen 127.0.0.1:8787
```

## API Endpoints

- `GET /healthz`
- `POST /sessions`
- `POST /sessions/{session_id}/ticks`
- `POST /sessions/{session_id}/approvals/{approval_id}`
- `GET /sessions/{session_id}/events?from_sequence=1&limit=200`
- `GET /sessions/{session_id}/events/stream?cursor=0&replay_limit=500`

## Key Workspace Paths

- Session root: `<root>/sessions/<session-id>/`
- Event log: `<root>/kernel/events/<session-id>.jsonl`
- Checkpoints: `<root>/sessions/<session-id>/checkpoints/`
- Tool reports: `<root>/sessions/<session-id>/tools/runs/`
