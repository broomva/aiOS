# Release Readiness

Use this checklist before production release/distribution.

## Reliability

1. Validate deterministic replay for representative workflows.
2. Validate crash recovery from mid-tick interruption.
3. Validate bounded retries and circuit-breaker behavior.

## Security

1. Validate capability enforcement and approval gates.
2. Validate secret redaction in logs/events.
3. Validate sandbox limits and command/network constraints.

## Observability

1. Ensure structured `tracing` spans for session, tick, and tool run boundaries.
2. Ensure metrics exist for latency, failures, backlog, and approval wait times.
3. Ensure incident-debuggable event provenance.

## Operations

1. Validate CI gates on default branch.
2. Validate release artifact reproducibility.
3. Validate deployment and rollback runbooks.

## Product Surface

1. Validate API contracts and migration notes for any breaking change.
2. Validate SSE replay behavior and reconnect semantics.
3. Validate docs and examples for operators and integrators.
4. Validate generated `/openapi.json` against OpenAPI schema.
