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
4. Linux ext4 documentation:
- URL: `https://docs.kernel.org/filesystems/ext4/index.html`
- Accessed: `2026-02-15`
- Influence: host filesystem baseline recommendation for local, durable journal/blob workloads.
5. Linux XFS documentation:
- URL: `https://docs.kernel.org/filesystems/xfs/index.html`
- Accessed: `2026-02-15`
- Influence: scale/performance tradeoff guidance for large artifact and parallel I/O workloads.
6. SQLite "Use Of SQLite On NFS":
- URL: `https://www.sqlite.org/useovernet.html`
- Accessed: `2026-02-15`
- Influence: explicit caution against network filesystem-backed embedded DB deployments for authoritative journal data.
7. Vercel AI SDK UI message stream protocol:
- URL: `https://ai-sdk.dev/docs/ai-sdk-ui/stream-protocol`
- Accessed: `2026-02-15`
- Influence: replay/stream framing requirements for event-to-UI adapters and protocol compatibility checks.
8. Vercel harness/filesystem work:
- URL: `https://vercel.com/blog/how-to-build-agents-with-filesystems-and-bash`
- URL: `https://vercel.com/blog/testing-if-bash-is-all-you-need`
- Accessed: `2026-02-15`
- Influence: harness simplification principles and filesystem-as-context evaluation criteria.
9. Can Boluk, "The Harness Problem":
- URL: `https://blog.can.ac/2026/02/12/the-harness-problem/`
- Accessed: `2026-02-15`
- Influence: edit-format reliability requirements and hashline-style anchoring relevance to tool correctness.
10. Lago repository:
- URL: `https://github.com/broomva/lago`
- Accessed: `2026-02-15`
- Influence: direct code-level evaluation of sequence assignment, branch semantics, replay behavior, and test coverage.

## Curation Rule

When adding a new external source, include:
1. URL and access date.
2. Why it matters.
3. Which design decision it influenced.
