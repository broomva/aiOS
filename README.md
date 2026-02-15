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

## Quality Gate

Run the same checks locally that CI runs:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Docs

- Architecture and crate boundaries: `docs/ARCHITECTURE.md`
