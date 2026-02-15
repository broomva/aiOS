# AGENTS.md

This file defines the agent workflow contract for this repository.

## Mission

Build and operate `aiOS` as a safe, event-native, session-oriented agent operating system in Rust.

## Non-Negotiable Rules

1. Preserve crate dependency direction:
- `aios-model` is foundational and side-effect free.
- Higher layers must depend downward only.
- Avoid cycles and "shortcut" imports across layers.

2. Route side effects through controlled boundaries:
- Tool execution flows through `aios-tools` + `aios-sandbox`.
- Session state changes flow through `aios-runtime`.
- Avoid hidden mutable state outside workspace files and event logs.

3. Keep event provenance and auditability intact:
- Emit events for meaningful transitions.
- Preserve monotonic sequence per session.
- Keep artifacts traceable to event history.

4. Enforce quality gate before completion:
- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `scripts/validate_openapi_live.sh`

5. Update docs with behavior changes:
- Update `README.md` for user-facing changes.
- Update `docs/ARCHITECTURE.md` for structural or model changes.
- Update relevant `context/` files for workflow changes.

6. Prefer additive, test-backed changes:
- Add or extend tests with every behavioral change.
- Do not silently widen capabilities or safety boundaries.

## Required Context Files

Read these first for non-trivial tasks:

1. `context/01-current-state.md`
2. `context/02-engineering-rules.md`
3. `context/03-agent-workflows.md`
4. `context/04-release-readiness.md`

## Project-Local Skills

Use these repo-local skills when their trigger conditions match:

1. `skills/kernel-evolution/SKILL.md`
- Use when changing core kernel behavior, event schema, capabilities, runtime lifecycle, checkpoints, or memory provenance.

2. `skills/control-plane-api/SKILL.md`
- Use when adding or modifying API endpoints, request/response contracts, SSE streaming, or approval APIs.

3. `skills/release-readiness/SKILL.md`
- Use when preparing production rollout, CI/CD hardening, packaging, observability, security controls, or distribution.

## Delivery Workflow

1. Load relevant `context/` and `skills/` files.
2. Define scope and invariants.
3. Implement changes in smallest coherent increments.
4. Add/adjust tests.
5. Run quality gate.
6. Summarize outcomes, risks, and next steps.

## Continuous Self-Learning

1. Extract lessons from completed tasks and write concise entries in `docs/INSIGHTS.md`.
2. Convert repeated workflows into reusable instructions by updating files in `skills/`.
3. Promote stable project knowledge into `context/` files to reduce repeated rediscovery.
4. Revisit assumptions after failures/incidents and record corrected guidance.

## Keep AGENTS.md Current

1. Update `AGENTS.md` when architecture, workflow, quality policy, or safety boundaries change.
2. Update `AGENTS.md` in the same change-set as the behavior change (do not defer).
3. Keep guidance minimal, actionable, and aligned with actual repository reality.
4. Treat stale AGENTS instructions as defects and fix immediately.

## Operational Defaults

1. Use `tracing` for runtime observability.
2. Prefer deterministic file paths under session workspace.
3. Keep policy default-deny for sensitive actions.
4. Keep public APIs version-aware and backward compatible unless explicitly changing contracts.
