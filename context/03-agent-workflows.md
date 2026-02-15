# Agent Workflows

## Standard Change Flow

1. Read `AGENTS.md` plus relevant files in `context/`.
2. Select relevant local skill(s) under `skills/`.
3. Confirm invariants before edits.
4. Implement minimal coherent changes.
5. Add/adjust tests.
6. Run quality gate.
7. Provide outcome summary including remaining risks.

## When to Use Which Skill

1. `skills/kernel-evolution/SKILL.md`
- Choose for core runtime behavior, event model changes, policy semantics, checkpointing, memory provenance.

2. `skills/control-plane-api/SKILL.md`
- Choose for API endpoint additions/changes, request/response contracts, SSE and replay behavior.

3. `skills/release-readiness/SKILL.md`
- Choose for production hardening, CI/CD improvements, observability, security posture, packaging and release planning.

## Definition of Done

1. Correctness verified by tests.
2. Lint and format gates pass.
3. Documentation reflects behavior.
4. No unresolved critical risks left implicit.
