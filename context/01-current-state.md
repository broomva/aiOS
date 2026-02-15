# Current State

## Repository Shape

- Workspace root has `apps/`, `crates/`, `docs/`, `.github/workflows/`.
- Runtime binaries:
  - `apps/aiosd`: local demo daemon.
  - `apps/aios-api`: HTTP control-plane and SSE stream server.

## Core Guarantees in Place

1. Session-oriented kernel runtime with mode switching and budget controllers.
2. Append-only event log with per-session sequencing and streaming.
3. Tool dispatch with policy evaluation and sandbox execution boundaries.
4. Workspace persistence of manifests, checkpoints, artifacts, tool reports, and memory observations.

## CI Baseline

`Rust CI` workflow enforces:
- format check
- clippy with warnings denied
- workspace tests
- locked dependency usage for check/clippy/test

## Immediate Gaps (as of now)

1. Deterministic replay equivalence is not fully enforced by tests.
2. Crash-recovery semantics need dedicated failure-injection coverage.
3. Strong sandbox backends (microVM/gVisor class) are not implemented yet.
4. Production metrics/telemetry and authn/authz are not complete.
