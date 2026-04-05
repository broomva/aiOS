---
name: release-readiness
description: Prepare aiOS for production release and distribution. Use when hardening CI/CD, observability, security boundaries, packaging, deployment, rollback strategy, and operational readiness.
---

# Release Readiness

1. Read `context/04-release-readiness.md` first.
2. Verify reliability controls: replay checks, recovery checks, and bounded failure handling.
3. Verify security controls: capability boundaries, approval gates, and sandbox constraints.
4. Verify observability controls: structured tracing and operational metrics.
5. Verify operational controls: reproducible builds, CI gates, and rollback plans.
6. Verify product surface: API contract stability, migration notes, and operator docs.
7. Track open release blockers explicitly and rank by severity.
