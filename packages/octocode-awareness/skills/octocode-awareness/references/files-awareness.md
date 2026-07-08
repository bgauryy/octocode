# Files-awareness - status, timestamps, collisions

Read this when multiple agents may touch the same local repo. It covers status, timestamps, and collision response. Per-command flags live in `coordination-protocol.md`; hook enforcement lives in `hooks.md`.

> **Pi tool mapping:** `workspace status` -> `workspace_status`; `lock acquire` -> `file_lock type:lock`; `refinement get` -> `memory_refine_get`; `signal list` -> `agent_signal action:list`. Pi `file_lock` also exposes `type:status` and `type:renew`; `agent_signal` also exposes `action:reply|ack|resolve`.

## `workspace status`

Run `workspace status` to read shared state at a glance: memory counts by lifecycle state, active tasks, unverified tasks, recent file locks, and open refinement counts.
Each lock shows `agent_id`, `file_path`, `acquired_at`, and `expires_at` (ISO-8601 UTC, newest first).

Flags: `--workspace` filters displayed locks/tasks under one workspace path; `--artifact` narrows to one package/service/artifact; `--limit` caps listed locks/pending tasks.
Expired locks are cleaned on each call and their tasks become `PENDING`, so what you see is live without erasing verification debt.

The status command reads the canonical global DB (`~/.octocode/memory/awareness.sqlite3` by default), filtered by `workspace_path`. It does not read generated `<repo>/.octocode/` files.

`workspace status` does not show refinement bodies. For handoff detail, run `refinement get`.
Refinements are keyed by `workspace_path`, optional `artifact`, and optional `repo`/`ref`; mismatched scope means `refinement get` can miss them.

To reason about task timing and ordering, combine three timestamp sources:
- **Lock times** (`acquired_at`/`expires_at` from `workspace status`) - when a claim was taken and when it lapses.
- **Record times** (`created_at`/`updated_at` on memories and refinements) - when knowledge or work state was last written.
- **File mtimes** - when a file actually changed on disk.

Read a file's mtime, or stamp a precise event time, with Node:

```bash
node -e "console.log(require('fs').statSync(process.argv[1]).mtime.toISOString())" path/to/file
node -e "console.log(new Date(performance.timeOrigin + performance.now()).toISOString())"
```

`performance.now()` is sub-millisecond but monotonic from process start, so always add `performance.timeOrigin` to get a comparable wall-clock instant.

## Collision protocol

A **collision** means another agent is actively working the same area.
Collision surfaces three ways: `lock acquire` returns `ok: false` with exit `2` on a needed file; `workspace status` shows a live lock; or `refinement get` shows an `ongoing` refinement.

When you detect one, do not silently steal the lock, force the edit, or quietly abandon the task.

1. Notify the user when reachable with concrete facts from the conflict payload or `workspace status`: who holds it (`agent_id`), since when (`acquired_at`), why (`rationale`/`test_plan`), and which files overlap.
2. Let the user decide: wait and retry, take a different slice, coordinate/hand off, or explicitly approve stale-lock cleanup.
3. If no user is reachable, wait/retry within a bounded budget or pick non-overlapping work, then record the collision so it is visible later.

Hooks enforce the mechanical half by blocking writes on exit `2`; this protocol is the human-in-the-loop half.

Use `lock wait` when waiting is the chosen path.
`lock wait` polls live locks, removes expired locks on each check, and exits `2` with current `conflicts[]` when the budget expires.
`lock wait` does not acquire a claim and sleeps outside SQLite transactions.
When `lock wait` exits `0`, immediately claim with `lock acquire` before editing.

If you need human/peer coordination during a longer wait, send `signal publish --kind request` or `--kind blocker`.
Target the holder or broadcast channel.
The `UserPromptSubmit` delivery hook surfaces that message on the other agent's next turn.

Use `lock prune --older-than-minutes 20 --dry-run` when a lock may be abandoned.
If the dry-run facts look right and cleanup is approved, run `lock prune --older-than-minutes 20`.
Stale cleanup releases files while preserving the task as `PENDING`; it is cleanup, not success.

## Per-repo/project + running-env context

Orient in a new session by combining the shared store with plain git:

- `workspace status` - memory counts, active/pending tasks, live locks.
- `refinement get` - open/ongoing handoffs for this repo/ref.
- `git status --porcelain` and `git rev-parse --abbrev-ref HEAD` - dirty files and branch; the store never mirrors the working tree.
- Refinements auto-fill `repo`/`ref` from the workspace's git when omitted; a workspace that is not a git repo stays unscoped rather than inheriting the caller's cwd repo.

Pass absolute paths or run from repo root.

## Observability

- `workspace status` - memory counts by state and label, active task count, open refinements, live locks.
- `query all --format html --out .octocode/awareness/index.html` - workspace `.octocode/` static browser view across memories, tasks, locks, agents, signals, files, activity, and repo profile.
- `reflect mine-weakness` - top recurring failure clusters by `failure_signature` (support x avg-importance).
- `maintenance digest --dry-run` - preview archive/prune counts before any cleanup mutates.
- `query memories --format markdown` - readable memory projection with labels, importance, context, observation, failure signature, and refs.
- Supersede lineage lives in `superseded_by` on each memory; recall `SUPERSEDED` rows with `memory recall --state SUPERSEDED` to trace it.

## Automatic session capture

The capture hook calls `session capture`. Claude-style hook hosts use `SessionEnd`; Codex uses `PreCompact` for best-effort capture before context changes. Cursor uses native `sessionEnd` locally plus `preCompact` for the cloud-supported compaction checkpoint.
`session capture` writes a work-handoff refinement from this session's locks plus the dirty git tree.
`session capture` no-ops on a clean tree with no session locks, skips on `clear`, is fail-open, and opts out via `OCTOCODE_NO_SESSION_CAPTURE=1`.
