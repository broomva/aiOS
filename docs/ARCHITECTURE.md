# aiOS Kernel Architecture (Rust)

This repository implements a microkernel-style agent operating system where **sessions** are the unit of execution and **side effects** are governed by policy, sandboxing, and durable events.

## Dependency Chain (Bottom -> Top)

1. `aios-model`
- Canonical data model and event schema.
- Defines: session manifest, capabilities, tool calls, event kinds, memory records, checkpoints, homeostasis state vector.
- No runtime side effects.

2. `aios-events`
- Append-only event log + subscription stream.
- Defines: `EventStore`, `FileEventStore`, `EventJournal`, stream hub.
- Depends on: `aios-model`.

3. `aios-policy`
- Capability evaluation, approval queue, session-scoped policy overrides.
- Defines: `PolicyEngine`, `SessionPolicyEngine`, `ApprovalQueue`.
- Depends on: `aios-model`.

4. `aios-sandbox`
- Constrained tool execution substrate.
- Defines: `SandboxRunner`, `LocalSandboxRunner`, limits and execution result.
- Depends on: `aios-model`.

5. `aios-tools`
- Tool registry + dispatcher.
- Built-in tool kinds: `fs.read`, `fs.write`, `shell.exec`.
- Dispatch flow: lookup -> policy check -> approval/deny -> execute via sandbox.
- Depends on: `aios-model`, `aios-policy`, `aios-sandbox`.

6. `aios-memory`
- Durable soul + observation store with provenance.
- Defines: `MemoryStore`, `WorkspaceMemoryStore`, `extract_observation`.
- Depends on: `aios-model`.

7. `aios-runtime`
- Kernel loop and control plane.
- Services: session manager, workspace bootstrap, event emission, tool execution orchestration, checkpoint/heartbeat, homeostasis loop.
- Depends on: `aios-model`, `aios-events`, `aios-tools`, `aios-memory`, `aios-policy`.

8. `aios-kernel`
- Composition root/builder for all services.
- Exposes a clean API: create session, tick loop, resolve approvals, subscribe events.
- Depends on: all runtime-facing crates.

9. `apps/aiosd`
- Demo daemon binary.
- Runs a bootstrap scenario and prints event stream.

## Session Workspace Layout

Each session is rooted at:

`<root>/sessions/<session-id>/`

Key files and directories:
- `manifest.json`
- `state/thread.md`
- `state/plan.yaml`
- `state/task_graph.json`
- `state/heartbeat.json`
- `checkpoints/<checkpoint-id>/manifest.json`
- `tools/runs/<tool-run-id>/report.json`
- `memory/soul.json`
- `memory/observations.jsonl`
- `artifacts/**`
- `inbox/human_requests/`
- `outbox/ui_stream/`

## Kernel Tick Lifecycle

Each tick executes:
1. `sense` (phase events + pending approvals)
2. `estimate` (state vector + operating mode)
3. `gate` (policy/approval)
4. `execute` (tool dispatch in sandbox)
5. `commit` (tool reports + file mutation events)
6. `reflect` (observation extraction + memory write)
7. `heartbeat` (budget update + checkpoint + state snapshot)
8. `sleep` (await next external signal)

## Homeostasis Model

State vector (`AgentStateVector`):
- `progress`
- `uncertainty`
- `risk_level`
- `budget` (`tokens/time/cost/tool calls/error budget`)
- `error_streak`
- `context_pressure`
- `side_effect_pressure`
- `human_dependency`

Controllers:
- **Uncertainty controller**: high uncertainty pushes mode toward `Explore`.
- **Error controller**: streak >= threshold trips `Recover` circuit breaker.
- **Budget controller**: every tool call decrements budgets.
- **Context controller**: pressure raises exploration/compression preference.
- **Side-effect controller**: high mutation pressure routes through `Verify`.

Modes:
- `Explore`
- `Execute`
- `Verify`
- `Recover`
- `AskHuman`
- `Sleep`

## Event-Native Streaming

All important state transitions become `EventKind` records. This supports:
- real-time UI streaming
- replay from cursor
- auditability and postmortems
- deterministic-enough recovery
