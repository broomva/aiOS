# Sources

This file captures source material and reference classes that influenced the architecture.

## Internal Sources

1. `docs/ARCHITECTURE.md`
2. `context/` files
3. `skills/` local skill definitions
4. Event schema and runtime implementation in `crates/`

## External Source Classes

1. Agent OS architecture discussions and control-plane patterns.
2. Event-sourced workflow and replay design patterns.
3. Capability-based security and sandboxing best practices.
4. Rust service reliability practices (`tracing`, strict linting, test-first hardening).

## Curation Rule

When adding a new external source, include:
1. URL and access date.
2. Why it matters.
3. Which design decision it influenced.
