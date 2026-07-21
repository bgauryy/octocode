# Coordinate

Load when managing live workers. Why: hosts differ in tool names; the **actions** are portable.

## Portable actions
Map these to the host API (Task tool, teammate/message APIs, A2A tasks, …):

| Action | Meaning |
|---|---|
| `list` | Inventory live workers and statuses |
| `status` | Inspect one worker without blocking |
| `wait` | Block until current turn is idle or terminal |
| `send` | Start next turn when idle |
| `followUp` | Queue work after the current turn |
| `steer` | Redirect mid-turn after current tool calls |
| `abort` | Stop the active turn; keep process if possible |
| `stop` / `kill` | Terminate; remove from registry when done |

## Rules
1. Spawn all independent workers before waiting on any.
2. Idle / “turn ended” ≠ acceptance — check the packet criteria.
3. Steer once on wrong direction; else stop and replan — do not replay the same packet.
4. Before concluding: `list`, reconcile failures, stop leftovers.
5. Worker registries are often in-memory — after session reload, spawn fresh (do not reuse stale ids).
6. Workers do not talk to the user unless a handoff packet says so.

## Sync vs async
- **Sync** — parent needs the result to continue → wait (or keep work in parent).
- **Async** — independent long work → spawn, continue, collect later.

Next: `synthesize.md` · `recovery.md`.
