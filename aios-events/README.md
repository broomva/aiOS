# aios-events

Event persistence and streaming primitives for `aiOS`.

## Responsibilities

- Append-only event storage (`EventStore`)
- File-backed store (`FileEventStore`)
- Journal facade (`EventJournal`)
- Broadcast stream hub for live subscribers

## Notes

Preserve per-session monotonic sequence semantics and replayability.
