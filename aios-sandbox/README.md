# aios-sandbox

Sandbox execution boundary for tool side effects.

## Responsibilities

- Sandbox request/limits model
- Runner abstraction (`SandboxRunner`)
- Local constrained runner (`LocalSandboxRunner`)

## Notes

All command execution should flow through this crate (or stronger future backends).
