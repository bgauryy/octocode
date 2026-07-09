# Files Awareness And Overlap

Every structured edit declares advisory `run_files` presence. Ordinary peers may
work on the same path; awareness makes that choice informed rather than silently
blocking it.

## On Overlap

1. Read the bounded peer packet: agent, task/run, short reason, exclusive state.
2. If changes are independent, continue; unchanged peer state will not repeat.
3. If changes interact, inspect `work show --file <path>` and signal the peer.
4. If sensitive exclusivity is needed, request it; acquisition fails until other live
   presence ends. Never surprise active peers with a lock.

```bash
octocode-awareness work show --workspace "$PWD" --file src/auth.ts --compact
octocode-awareness signal publish --agent-id "$OCTOCODE_AGENT_ID" \
  --workspace "$PWD" --kind request --subject "Coordinate auth.ts" \
  --file src/auth.ts --compact
```

## On Exclusive Conflict

Stop before editing. Preserve holder, run/task, reason, heartbeat/expiry. Choose
bounded wait, direct coordination, another task/file, or expired cleanup.

`lock wait` observes only; acquire after clear. Preview `lock prune --dry-run` before
cleanup. Expiry removes coordination, never proves completion.

## Coverage

Hooks cover recognized write payloads. Arbitrary shell/external writes may only be
found by dirty-tree reconciliation; without hooks, call `work start|touch` manually.
Keep one normalized workspace and absolute operational paths so the same file joins.

Task paths are non-exclusive planning scope. Run files are live work. Locks are
exclusive safety. Edit log is completed history; do not conflate these four layers.

Command/verification detail: `lock-protocol.md`; timing/handoffs:
`session-observability.md`.
