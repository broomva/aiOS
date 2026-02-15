# aios-policy

Capability policy engine and approval queue.

## Responsibilities

- Capability evaluation (`PolicyEngine`)
- Static and per-session policy resolution
- Approval ticket lifecycle (`ApprovalQueue`)

## Notes

Default to least privilege and require explicit approval for gated capabilities.
