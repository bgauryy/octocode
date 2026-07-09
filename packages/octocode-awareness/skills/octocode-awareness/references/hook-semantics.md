# Hook Runtime Semantics

Identity order: environment agent ID, payload agent, payload session, warned
host/workspace fallback. Export one stable `OCTOCODE_AGENT_ID`.

## Write Path

1. Extract deduplicated paths; no paths -> no-op.
2. Evaluate harness guard before any DB presence.
3. Resolve exactly one TASK claim, matching explicit WORK presence, or create HOOK
   fallback. Never reuse by session alone.
4. Declare advisory work. Existing exclusive blocks; ordinary peers succeed.
5. Emit peer context only when its fingerprint changes.
6. Post-edit logs/heartbeats. TASK/WORK stays active; HOOK ends PENDING.

Infrastructure/input failure warns and fails open. Guard denial and real exclusive
conflict block with exit 2. Correlation loss never marks success.

## Host Edges

| Edge | Claude/Codex | Cursor | Pi |
|---|---|---|---|
| Before | PreToolUse | preToolUse | tool call/start |
| After | PostToolUse | postToolUse | tool result/end |
| Brief | UserPromptSubmit | sessionStart | before agent start |
| Verify | Stop/SubagentStop | stop/subagentStop | bounded agent-end reminder |
| Capture | SessionEnd/PreCompact | sessionEnd/preCompact | shutdown/pre-compact |

Briefing/peer/session delivery fingerprints suppress unchanged context without
acknowledging signals. Verification output caps three rows plus omitted count.

Presence/task claim TTLs are independent. Expiry removes stale coordination, never
success, and never changes a live TASK run to PENDING.

Tuning/installation belongs to `hooks.md`; file decisions to `files-awareness.md`.
