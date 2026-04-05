# Filesystem and Harness Evaluation

Date: 2026-02-15

## Scope

1. Define the optimal filesystem strategy for an agent operating system.
2. Evaluate the harness problem impact on filesystem/tool design.
3. Evaluate Lago as a candidate persistence substrate for aiOS.

## Decision Summary

There is no single "best" filesystem for agent workloads. The best result is a layered design:

1. Event journal (authoritative timeline)
2. Content-addressed blob store (immutable bytes)
3. Manifest projection (current tree view)
4. Materialized workspace view (for tools and humans)
5. Checkpoints/snapshots (fast recovery and replay)

This architecture matches agent constraints better than relying on a mutable directory tree as the source of truth.

## Host Filesystem Guidance

For the host backing store:

1. Default production choice: `ext4` on local NVMe.
2. High-throughput large-file/artifact workloads: consider `XFS`.
3. Avoid shared network filesystems for embedded journal databases (NFS/SMB) unless using a storage design explicitly built for distributed locking semantics.

Rationale:

1. `ext4` provides mature journaling and robust metadata crash recovery.
2. `XFS` is optimized for scale and parallel metadata/data operations.
3. SQLite documentation repeatedly warns that lock reliability on network filesystems can corrupt databases.

## Harness Problem Implications

The harness is the dominant reliability boundary between model intent and side effects. Filesystem design must support harness-safe operations:

1. Stable edit anchors (hashline-style line identity) to reduce brittle patch/replace failures.
2. Full side-effect auditability (event log first, not optional logs).
3. Deterministic-enough replay from event provenance.
4. Streaming-first event protocol for UI and operators.
5. Capability-gated writes and sandboxed execution paths.

In practical terms: the model proposes, the harness executes, and the filesystem/event layer proves what happened.

## Lago Evaluation

## What Lago already does well

1. Event-sourced journal with embedded ACID storage (`redb`) and append/read/stream primitives.
2. Content-addressed blob storage (SHA-256 + compressed blobs).
3. Virtual filesystem projection from events (`FileWrite`, `FileDelete`, `FileRename`).
4. Branching and diff model in `lago-fs`.
5. Multi-format SSE adapters, including Vercel AI SDK v6 style parts/header.
6. Strong baseline test coverage (`cargo test --workspace` passed locally in `/tmp/lago-src`).

## Gaps and correctness risks found

1. Sequence assignment mismatch:
- API routes emit some events with `seq: 0` and comments imply journal assignment.
- `RedbJournal` writes using caller-provided `event.seq` and does not assign/validate monotonic sequence.
- Risk: duplicate sequence keys can overwrite prior event records for the same `(session, branch, seq)` tuple.

2. Branch-aware filesystem APIs are incomplete:
- `write_file`/`patch_file`/`delete_file` currently hardcode `"main"` branch.
- Manifest builder queries by session without branch filter in API route helper.
- Risk: branch semantics are not consistently enforced by file endpoints.

3. SSE route behavior is tail-only:
- Event stream endpoint opens a tail stream and does not replay existing records on initial subscribe.
- This weakens resumability and deterministic reconstruction expectations for clients.

4. Documentation drift:
- Harness roadmap notes Vercel protocol update as future work, but code already implements UI message stream header/parts.

## Fit Assessment for aiOS

Lago is a strong candidate for aiOS persistence core if the sequence/branch/replay gaps are fixed before adopting it as the canonical session substrate.

## Recommended Integration Strategy (Phased)

1. Phase A: Treat Lago as a storage adapter behind aiOS `EventStore` and workspace manifest interfaces.
2. Phase B: Enforce journal-side sequence assignment and reject duplicate/out-of-order appends.
3. Phase C: Make branch an explicit parameter on all file APIs and manifest queries.
4. Phase D: Add replay-first SSE mode (cursor + bounded replay before live tail).
5. Phase E: Add cross-project contract tests validating:
- append monotonicity
- replay determinism
- branch isolation
- Vercel v6 stream conformance

## How We Know We Are Moving in the Right Direction

Track and gate releases on measurable invariants:

1. Replay determinism:
- same event prefix + same tool outputs => identical projected manifest hash

2. Branch isolation:
- writes on branch `B` do not change manifest hash for branch `A`

3. Sequence safety:
- no duplicate `(session, branch, seq)` keys accepted

4. Harness reliability:
- edit failure rate and retry-loop rate trend downward over benchmark suite

5. Side-effect auditability:
- every artifact/tool output cites originating event IDs and file hashes

6. Recovery correctness:
- crash/restart fault-injection tests restore last committed checkpoint without duplicate effects

These should be enforced as CI checks, not just dashboards.
