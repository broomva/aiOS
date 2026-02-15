# Contributing

## Workflow

1. Read `AGENTS.md` before making non-trivial changes.
2. Follow relevant files under `context/` and `skills/`.
3. Keep changes small, test-backed, and architecture-aligned.

## Quality Gate

Run before opening a PR:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Documentation

Update docs when behavior changes:
- `README.md` for user-facing behavior.
- `docs/ARCHITECTURE.md` for system/model changes.
- `context/` files for workflow or readiness changes.
