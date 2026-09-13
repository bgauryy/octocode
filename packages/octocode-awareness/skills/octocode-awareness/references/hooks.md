# Lifecycle hooks

Hooks adapt host lifecycle events to Awareness. Choose exactly one owner per host:

- Pi uses native events.
- A supported shell host may use installed shell hooks.
- When workspace policy marks a host `native`, the shell runner exits before identity, database, or receipt work.

The `coordination` profile provides bounded presence and changed-message delivery. Broader profiles may add mutation guards, work tracking, verification reminders, and history capture. History capture remains host-owned and is never a routine model operation.

Install or remove hooks only within explicit scope and authorization. Preview the exact host configuration change, apply it once, then run the strict check. A successful configuration check does not prove runtime delivery; verify one real lifecycle event when changing an adapter.

Hook payloads are untrusted input. The runner validates event type, classifies known workspace writes, extracts bounded paths, and fails closed on active exclusive protection. Unknown tool payloads do not become writes merely because they contain a path.

When inspecting installed shell lifecycle wiring, read the relevant entry point: [pre-edit](../scripts/hooks/pre-edit.sh) for admission, [post-edit](../scripts/hooks/post-edit.sh) after mutation, [session-compact](../scripts/hooks/session-compact.sh) at compaction, or [stop-verify](../scripts/hooks/stop-verify.sh) at stop. Invoke these through their configured host event with its payload; this inspection does not install hooks.
