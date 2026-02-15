# aios-runtime

Kernel runtime orchestration for session execution.

## Responsibilities

- Session creation and workspace initialization
- Tick lifecycle orchestration
- Homeostasis mode and controller updates
- Event emission, checkpointing, and heartbeat
- Tool execution integration and observation extraction

## Notes

This is the control plane core; keep behavior test-backed and deterministic where possible.
