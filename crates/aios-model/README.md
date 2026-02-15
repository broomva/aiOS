# aios-model

Canonical domain model for `aiOS`.

## Responsibilities

- Core identifiers and manifests (`SessionId`, `SessionManifest`)
- Capability and policy model
- Event schema (`EventKind`, `EventRecord`)
- Homeostasis state types (`AgentStateVector`, `OperatingMode`, `BudgetState`)
- Memory and checkpoint model

## Constraints

- Side-effect free
- Must remain dependency-foundational for other crates
