# Status

Last updated: 2026-02-15

## Build and Quality

- Workspace builds successfully.
- CI runs format, clippy (`-D warnings`), and tests.
- Core unit/integration tests are present for policy and kernel flows.

## Implemented

1. Core kernel layering (`aios-model` -> `aios-kernel`).
2. Session runtime with event-native lifecycle.
3. Capability policy and approval queue.
4. Sandbox execution boundary (local constrained runner).
5. Tool registry/dispatcher with initial built-in tools.
6. Workspace persistence for manifests/checkpoints/tool reports/memory observations.
7. Control-plane HTTP API and SSE replay/live streaming.
8. Voice ingress first slice: voice session start, websocket audio stream loopback, and voice event types.
9. Vercel AI SDK v6 UIMessage stream adapter endpoint with typed data-part mapping.
10. OpenAPI 3.1 spec endpoint and Scalar interactive docs route.
11. CI OpenAPI schema validation and pre-commit/pre-push hook configuration.
12. Filesystem + harness architecture evaluation with Lago fit/gap analysis.
13. Strict per-session sequence monotonicity enforcement in the event store, with stream gap backfill on lagged/live SSE paths (native and Vercel v6).
14. Branch-aware event model (`branch_id`) with branch-scoped sequences, reads, and SSE stream filtering/querying.
15. Branch lifecycle hardening: fork-sequence bounds validation, merge-safety rules, and merged-branch read-only enforcement.
16. Structured `tracing` spans across kernel/runtime/tool/sandbox/event-store boundaries for operator-grade execution visibility.

## In Progress / Partial

1. Replay determinism guarantees are improving (sequence + backfill guards in place) but still partial end-to-end.
2. Crash-recovery tests need explicit failure-injection scenarios.
3. Metrics and SLO dashboards beyond structured logs/traces are limited.

## Not Yet Implemented

1. Strong sandbox backends (microVM/gVisor class).
2. Multi-tenant authn/authz and RBAC.
3. Distributed scheduler/backpressure control-plane.
4. Production packaging/release artifacts and signed provenance pipeline.
