# aios-kernel

Composition root and public kernel API.

## Responsibilities

- Wire events, policy, sandbox, tools, memory, and runtime
- Expose ergonomic APIs:
  - `create_session`
  - `tick`
  - `resolve_approval`
  - `read_events`
  - `subscribe_events`

## Notes

Use this crate as the main embedding surface for applications/services.
