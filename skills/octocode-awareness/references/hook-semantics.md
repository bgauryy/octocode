# Hook Runtime Semantics
Identity order: host payload agent ID, environment agent ID, payload session, warned
host/workspace fallback. Export one stable `OCTOCODE_AGENT_ID`.

## Write Path

1. Extract deduplicated paths; no paths -> no-op.
2. Evaluate harness guard before any DB presence.
3. Resolve exactly one TASK claim, matching explicit WORK presence, or the active
   fallback for the same agent + stable session/transcript + workspace + artifact.
4. Declare advisory work. Existing exclusive blocks; ordinary peers succeed.
5. Emit peer context only when its fingerprint changes.
6. Post-edit logs/heartbeats. TASK/WORK stays active; scoped HOOK stays active.
7. Stop, PreCompact, or SessionEnd finalizes scoped HOOK runs once; Stop audits debt.

N edits in one scoped turn produce one PENDING HOOK with N files. TASK/WORK never
merge into it. Shell creation is cross-process locked; Pi coalesces synchronous
in-process callbacks. Missing stable session correlation uses isolated fallback.
Recursive Stop surfaces newly finalized continuation debt once, then permits an
unchanged recursive Stop to avoid a host loop. Verification plans stay bounded and
require a relevant check, diff inspection, and recorded result.

Infrastructure/input failure warns and fails open. Guard denial and real exclusivity
use the host's native block shape: exit 2 for Claude/Codex; `permission: deny` for
Cursor; `{ block: true }` for Pi. Correlation loss never marks success.

## Host Edges

| Edge | Claude/Codex | Cursor | Pi |
|---|---|---|---|
| Before | PreToolUse | preToolUse | tool call/start |
| After | PostToolUse | postToolUse | tool result/end |
| Brief | UserPromptSubmit | sessionStart | before agent start |
| Verify | Stop/SubagentStop | stop/subagentStop | bounded agent-end reminder |
| Finalize/capture | SessionEnd (Claude) or PreCompact (Codex, no SessionEnd) | sessionEnd/preCompact | shutdown/pre-compact |

Claude/Codex context uses event-named `hookSpecificOutput`. Cursor uses
`additional_context` at session start and `agent_message` around tool use, but host
delivery remains best-effort and must be smoked. Cursor stop uses `followup_message`;
Claude/Codex stop uses exit 2. Fingerprints suppress unchanged context without
acknowledging signals. Verification output caps three rows plus omitted count.

PreCompact finalizes/captures but keeps the host session reusable. SessionEnd and Pi
shutdown mark the session ended; they do not delete explicit WORK or claim success.

Presence/task claim TTLs are independent. Expiry removes stale coordination, never
success, and never changes a live TASK run to PENDING.

Tuning/installation belongs to `hooks.md`; file decisions to `files-awareness.md`.
