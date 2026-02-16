# Insights

Track design ideas, operational learnings, and architectural insights.

## Current Insights

1. Event-native design makes replay, auditing, and streaming composable from the same primitive.
2. Session is the correct execution unit for agent work; process-level analogies are insufficient.
3. Homeostasis controllers (uncertainty/budget/error) reduce thrashing and unsafe action escalation.
4. Keeping tool side effects behind capability + sandbox boundaries is essential for trustworthy automation.
5. Reusing one OpenAPI live-validation script across CI and pre-push hooks prevents contract drift between local and hosted checks.
6. "Optimal filesystem" for agents is a layered substrate (journal + blob CAS + manifest projection + workspace view), not a single host FS choice.
7. Stream correctness depends on strict sequence invariants plus gap backfill; live broadcast alone is insufficient under lag.
8. Branch-scoped sequencing is required for safe multi-path exploration; global per-session sequence counters hide branch correctness bugs.
9. Branch lifecycle needs explicit guards (fork bounds, merge-source constraints, and merged-branch read-only rules) to keep replay behavior predictable.
10. Distributed tracing only becomes actionable when spans cross boundaries (kernel -> runtime -> tool dispatcher -> sandbox -> event store), not when scoped to HTTP alone.

## Working Ideas

1. Add replay diffing as a first-class CI report artifact.
2. Introduce policy simulation mode (`dry-run`) for risky tool calls.
3. Add a structured incident timeline generator from event logs.

## Update Rule

When a significant behavior change or incident occurs:
1. Add a concise entry with date, context, and outcome.
2. Reference impacted files/endpoints.
3. Note whether AGENTS/context/skills were updated accordingly.
