---
name: kernel-evolution
description: Evolve core aiOS kernel behavior and contracts. Use when modifying crates aios-model, aios-events, aios-policy, aios-tools, aios-memory, aios-runtime, or aios-kernel, especially for event schema, lifecycle phases, capability checks, checkpoints, memory provenance, or session semantics.
---

# Kernel Evolution

1. Read `context/01-current-state.md` and `context/02-engineering-rules.md` first.
2. Preserve dependency direction and keep lower layers side-effect free.
3. Define invariants before edits (event ordering, policy behavior, replay assumptions).
4. Implement smallest coherent change across affected crates.
5. Add tests at the layer where behavior is introduced and at integration boundaries where behavior is consumed.
6. Update docs if data models, lifecycle phases, or guarantees change.
7. Run the quality gate:
- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
