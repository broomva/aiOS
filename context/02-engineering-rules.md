# Engineering Rules

## Rust Quality Bar

1. Keep `unsafe` out unless explicitly justified and reviewed.
2. Treat clippy warnings as errors in normal workflows.
3. Keep APIs small and typed; avoid stringly-typed contracts when a struct/enum is viable.
4. Return structured errors with actionable context.

## Test Expectations

1. Add unit tests for pure logic (policy matching, state transitions, parsing).
2. Add integration tests for cross-crate behavior (tick lifecycle, API contracts).
3. Add regression tests for every bug fix.

## Architecture Discipline

1. Preserve microkernel layering and dependency direction.
2. Keep side-effectful work inside explicit execution boundaries.
3. Avoid hidden global mutable state.
4. Persist meaningful state transitions as events.

## API and Compatibility

1. Keep JSON contracts stable by default.
2. Additive changes are preferred; breaking changes require explicit migration notes.
3. Validate and sanitize all external input.

## Security Defaults

1. Default to least privilege in capability checks.
2. Preserve approval gates for elevated or destructive actions.
3. Redact secrets from logs/events/context.
4. Keep network egress and command execution constrained by policy.
