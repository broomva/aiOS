# Sources

This file captures source material and reference classes that influenced the architecture.

## Internal Sources

1. `docs/ARCHITECTURE.md`
2. `context/` files
3. `skills/` local skill definitions
4. Event schema and runtime implementation in `crates/`

## External Source Classes

1. Agent OS architecture discussions and control-plane patterns.
2. Event-sourced workflow and replay design patterns.
3. Capability-based security and sandboxing best practices.
4. Rust service reliability practices (`tracing`, strict linting, test-first hardening).

## External References

1. Vercel AI SDK UIMessage stream protocol docs:
- URL: `https://ai-sdk.dev/docs/ai-sdk-ui/stream-protocol`
- Accessed: `2026-02-15`
- Influence: pinned the control-plane interface contract to Vercel AI SDK v6 framing and
  header semantics (`x-vercel-ai-ui-message-stream: v1`) while keeping kernel-native events
  as the source of truth.
2. Scalar docs:
- URL: `https://docs.scalar.com/`
- Accessed: `2026-02-15`
- Influence: interactive API docs embedding via `@scalar/api-reference` under `/docs`.
3. openapi-spec-validator:
- URL: `https://github.com/python-openapi/openapi-spec-validator`
- Accessed: `2026-02-15`
- Influence: CI and hook-based schema validation for generated `/openapi.json`.

## Curation Rule

When adding a new external source, include:
1. URL and access date.
2. Why it matters.
3. Which design decision it influenced.
