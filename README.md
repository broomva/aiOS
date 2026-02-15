# aiOS

`aiOS` is a Rust-based agent operating system scaffold focused on:
- session-oriented execution
- append-only event logs
- capability-governed tool calls
- sandboxed side effects
- memory with provenance
- homeostasis (mode switching + budgets + circuit breakers)

## Quick Start

```bash
cargo run -p aiosd -- --root .aios
```

This runs a demo kernel session and executes three ticks:
1. write an artifact (`fs.write`)
2. execute a bounded shell command (`shell.exec`)
3. read an artifact (`fs.read`)

Event records are streamed to logs as they are appended.

Run the HTTP control plane:

```bash
cargo run -p aios-api -- --root .aios --listen 127.0.0.1:8787
```

Core endpoints:
- `GET /openapi.json`
- `GET /docs` (Scalar interactive docs)
- `POST /sessions`
- `POST /sessions/{session_id}/ticks`
- `GET /sessions/{session_id}/events`
- `GET /sessions/{session_id}/events/stream?cursor=0` (SSE replay + live tail)
- `GET /sessions/{session_id}/events/stream/vercel-ai-sdk-v6?cursor=0` (Vercel AI SDK v6 UIMessage stream protocol)
- `POST /sessions/{session_id}/voice/start`
- `GET /sessions/{session_id}/voice/stream?voice_session_id=...` (WebSocket)

## Quality Gate

Run the same checks locally that CI runs:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
scripts/validate_openapi_live.sh
```

## Pre-Commit Hooks

This repository includes `.pre-commit-config.yaml` with:
- `pre-commit`: `cargo fmt --all --check`
- `pre-push`: OpenAPI validation, clippy, and tests

Install:

```bash
python3 -m venv .venv
source .venv/bin/activate
python -m pip install pre-commit openapi-spec-validator==0.7.2
pre-commit install --hook-type pre-commit --hook-type pre-push
```

## Docs

- Architecture and crate boundaries: `docs/ARCHITECTURE.md`
- Docs index: `docs/README.md`
- Current status: `docs/STATUS.md`
- Roadmap: `docs/ROADMAP.md`
- Technical reference: `docs/REFERENCE.md`
- Sources: `docs/SOURCES.md`
- Ideas and insights: `docs/INSIGHTS.md`
- Agent workflow contract and rules: `AGENTS.md`
- Context bundle: `context/`
- Project-local skills: `skills/`
