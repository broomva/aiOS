# aios-runtime

Kernel runtime orchestration for session execution.

## Responsibilities

- Session creation and workspace initialization
- Tick lifecycle orchestration
- Ordered turn middleware composition via `TurnMiddleware` and `TurnContext`
- Per-turn tool-call guard evaluation for middleware-installed safety controls
- Homeostasis mode and controller updates
- Event emission, checkpointing, and heartbeat
- Tool execution integration and observation extraction

## Notes

`KernelRuntime::tick_on_branch` now executes through an ordered middleware chain before
entering the terminal turn executor. Middleware can rewrite the turn envelope
(`TickInput`, estimated mode, state vector, pending approvals) without bypassing the
canonical event-emitting execution path.

Middleware can also install `ToolCallGuard`s into the turn context. Guards run after the
model emits a tool call but before policy evaluation or execution, which is where loop
detection now lives. `LoopDetectionMiddleware` persists warning/hard-stop events and can
force a text-only response once repeated tool-call signatures exceed the configured limit.

This is the control plane core; keep behavior test-backed and deterministic where possible.
