# Files-awareness — status, timestamps, collisions

Read this when multiple agents may touch the same local repo. It covers status, timestamps, and collision response. Per-command flags live in `coordination-protocol.md`; hook enforcement lives in `hooks.md`.

> **Pi tool mapping:** `status` → `workspace_status`; `pre-flight-intent` → `file_lock type:lock`; `refine-get` → `memory_refine_get`; `notify-get` → `agent_signal action:list`.

## `workspace_status` (Pi) / `status` (CLI)

Run `status` to read shared state at a glance: memory counts by lifecycle state, active intents, unverified intents, and recent file locks.
Each lock shows `agent_id`, `file_path`, `acquired_at`, and `expires_at` (ISO-8601 UTC, newest first).
Status shows who is editing now. Status also shows which edits still owe verification.

Flags: `--workspace` filters displayed locks/intents under one workspace path; `--limit` caps listed locks/pending intents.
Expired locks are cleaned on each call and their intents become `PENDING`, so what you see is live without erasing verification debt.

`status` shows memories, intents, and locks, but **not** refinements.
For the work-handoff view, run `refine-get` separately.
Everything lives in one shared store now, so a handoff can't "land in the wrong file."
Refinements are still keyed by `repo`/`ref`; mismatched scope means `refine-get` will miss them.

To reason about task timing and ordering, combine three timestamp sources:
- **Lock times** (`acquired_at`/`expires_at` from `status`) — when a claim was taken and when it lapses.
- **Record times** (`created_at`/`updated_at` on memories and refinements) — when knowledge or work state was last written.
- **File mtimes** — when a file actually changed on disk.

Read a file's mtime, or stamp a precise event time, with Node:

```bash
node -e "console.log(require('fs').statSync(process.argv[1]).mtime.toISOString())" path/to/file
node -e "console.log(new Date(performance.timeOrigin + performance.now()).toISOString())"
```

`performance.now()` is sub-millisecond but monotonic from process start, so always add `performance.timeOrigin` to get a comparable wall-clock instant. Use these to judge whether another agent's lock or note is fresh or stale before acting on it.

## Collision protocol

A **collision** means another agent is actively working the same area.
Collision surfaces three ways: `pre-flight-intent` returns `ok: false` / exit `2` on a needed file; `status` shows a live lock; or `refine-get` shows an `ongoing` refinement.

When you detect one, do **not** silently steal the lock, force the edit, or quietly abandon the task. Instead:
1. **Notify the user** (when a user is reachable) with the concrete facts from the conflict payload / `status`: who holds it (`agent_id`), since when (`acquired_at`), why (`rationale`/`test_plan`), and which files overlap.
2. **Let the user decide**: wait and retry, take a different slice that doesn't overlap, coordinate/hand off, or explicitly approve stale-lock cleanup.
3. If no user is reachable (headless/automated run), fall back to the safe default — wait/retry within a bounded budget or pick non-overlapping work — and record the collision so it is visible later.

The hooks enforce the mechanical half (they block the write on exit `2`); this protocol is the human-in-the-loop half the agent must add on top.

Use `wait-for-lock` when "wait" is the chosen path.
`wait-for-lock` polls live locks, removes expired locks on each check, and exits `2` with current `conflicts[]` when the budget expires.
`wait-for-lock` does **not** acquire a claim and sleeps outside SQLite transactions, so a waiter cannot deadlock the holder.
When it exits `0`, immediately claim with `pre-flight-intent` before editing.

If you need human/peer coordination during a longer wait, send a `notify --kind request` or `blocker`.
Target the holder or broadcast channel.
The `UserPromptSubmit` delivery hook surfaces that message on the other agent's next turn.
The `PostToolUse` hook/TTL is still the mechanical unblock signal.

Use `prune-stale-locks --older-than-minutes 20 --dry-run` when a lock may be abandoned.
If the dry-run facts look right, run `scripts/prune-stale-locks.sh 20` or the direct command without `--dry-run`.
Stale cleanup releases files while preserving the intent as `PENDING`; it is cleanup, not success.

## Per-repo/project + running-env context

- **`env`** — first orient command in a new session.
  Reports runtime, cwd, git state, changed files, open handoffs, and unverified intents.
- `env.git.changed_files` is the exact dirty-file count.
  `env.git.changes[]` is a bounded changed-file list.
  `github_url` is `null` when origin, branch, or path cannot map to GitHub.
- Refinements **capture the running env at write time** and auto-fill `repo`/`ref` from git when omitted.
  The next agent can compare environments before trusting a handoff.
  Use `refine-get --include-env` only for full file-change detail.
  Pass absolute paths or run from repo root.

## Observability

- **`stats`** — harness-health ledger: memories by state/importance, supersede churn, stale-ACTIVE count, top recurring weaknesses, and refinements by state×quality. Read it to decide what to prune or fix.
- **`memory-graph [--format mermaid|dot]`** — serializes the `superseded_by` lineage to stdout (paste into mermaid.live or `dot`); no server.

## Automatic session capture (`session-capture` / SessionEnd hook)

The `SessionEnd` hook calls `session-capture`.
`session-capture` writes a work-handoff refinement from this session's locks plus the dirty git tree.
`session-capture` no-ops on a clean tree with no session locks, skips on `clear`, is fail-open, and opts out via `OCTOCODE_NO_SESSION_CAPTURE=1`.
