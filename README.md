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
- `POST /sessions`
- `POST /sessions/{session_id}/ticks`
- `GET /sessions/{session_id}/events`
- `GET /sessions/{session_id}/events/stream?cursor=0` (SSE replay + live tail)

## Quality Gate

Run the same checks locally that CI runs:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
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
