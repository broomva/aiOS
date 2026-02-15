---
name: control-plane-api
description: Build or modify aiOS control-plane HTTP/SSE behavior. Use when changing apps/aios-api routes, request/response models, streaming/replay semantics, approval endpoints, or API compatibility guarantees.
---

# Control Plane API

1. Read `context/02-engineering-rules.md` and `context/03-agent-workflows.md` first.
2. Keep request/response contracts explicit and typed.
3. Keep additive compatibility by default; document intentional breaking changes.
4. Validate input strictly and return structured errors.
5. Ensure replay and live stream behavior remain consistent (`cursor` semantics, monotonic event ids).
6. Add API-level tests for parsing and contract regressions.
7. Update `README.md` endpoint documentation when route behavior changes.
8. Run the quality gate:
- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
