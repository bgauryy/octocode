# Files-awareness — status, timestamps, collisions

Read this when multiple agents may touch the same local repo. It covers how to see what others are doing (`status` + timestamps) and what to do when you collide with another agent. The per-command flag reference lives in `coordination-protocol.md`; the automatic hook enforcement lives in `hooks.md`.

## `status` & files-awareness

Run `status` to read the shared state at a glance — memory counts by lifecycle state, active intents, unverified intents, and the most recent file locks, each with `agent_id`, `file_path`, `acquired_at`, and `expires_at` (ISO-8601 UTC, newest first). This is how you tell what other agents in the same local repo are working on right now and which finished edits still owe verification; `--workspace` filters displayed locks/intents under one workspace path and `--limit` caps listed locks/pending intents. Expired locks are cleaned on each call and their intents become `PENDING`, so what you see is live without erasing verification debt.

`status` shows memories, intents, and locks, but **not** refinements — for the work-handoff view, run `refine-get` separately. Everything is in the one shared store now, so a handoff can't "land in the wrong file"; but it is keyed by `repo`/`ref`, so a mismatched `--repo`/`--ref` (or a different cwd that auto-detects a different repo/branch) means the next agent's `refine-get` won't find it. Double-check `repo`/`ref` when a handoff "disappears."

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

A **collision** is when another agent is actively working the same area you are — surfaced three ways: `pre-flight-intent` (or the pre-edit hook) returns `ok: false` / exit `2` on a file you need; `status` shows a live lock another agent holds on your target; or `refine-get` shows an `ongoing` refinement another agent owns on the same files/area.

When you detect one, do **not** silently steal the lock, force the edit, or quietly abandon the task. Instead:
1. **Notify the user** (when a user is reachable) with the concrete facts from the conflict payload / `status`: who holds it (`agent_id`), since when (`acquired_at`), why (`rationale`/`test_plan`), and which files overlap.
2. **Let the user decide**: wait and retry, take a different slice that doesn't overlap, coordinate/hand off, or explicitly approve stale-lock cleanup.
3. If no user is reachable (headless/automated run), fall back to the safe default — wait/retry within a bounded budget or pick non-overlapping work — and record the collision so it is visible later.

The hooks enforce the mechanical half (they block the write on exit `2`); this protocol is the human-in-the-loop half the agent must add on top.

Use `wait-for-lock` when "wait" is the chosen path. It polls the live lock table, removes expired locks on each check, and exits `2` with the current `conflicts[]` when the budget expires. It does **not** acquire a claim, and it sleeps outside SQLite transactions, so a waiter cannot deadlock the holder. When it exits `0`, immediately claim with `pre-flight-intent` before editing.

If you need human/peer coordination during a longer wait, send a `notify --kind request` or `blocker` to the holder or broadcast channel. The `UserPromptSubmit` delivery hook surfaces that message on the other agent's next turn; the `PostToolUse` hook/TTL is still the mechanical unblock signal.

Use `prune-stale-locks --older-than-minutes 20 --dry-run` when a lock may be abandoned. If the dry-run facts look right, run `scripts/prune-stale-locks.sh 20` or the direct command without `--dry-run`. This releases files while preserving the intent as `PENDING`; it is cleanup, not success.

## Per-repo/project + running-env context

- **`env`** — your first orient command in a new session. Reports the running environment (OS/platform, Python/Node versions, cwd), the detected **git repo / branch / dirty** state, changed-file metadata, the **open work-handoff for this repo** (`open`/`ongoing` refinements), and any **unverified intents**. Use it to answer "where am I, what's pending here, and what env am I in."
- `env.git.changed_files` is the exact dirty-file count. `env.git.changes[]` is a bounded list (first 200) of `{path,status,index_status,worktree_status,branch,github_url}` plus `previous_path` for renames/copies; `github_url` is `null` when origin is not GitHub, the branch is detached/unknown, or the path is new/untracked/renamed/copied and may not exist on the current GitHub branch yet.
- Refinements **capture the running env at write time** (`env_json` → `env` summary by default), and auto-fill `repo`/`ref` from git when you don't pass them — so the next agent sees whether the environment differs before trusting a handoff. Use `refine-get --include-env` only when the full file-change list matters. Pass absolute paths or run from the repo root so file correlation holds.

## Observability

- **`stats`** — a harness-health ledger, not just counts: memories by state/importance, **supersede churn**, **stale-ACTIVE** count (`--stale-days`), **top recurring weaknesses**, and refinements by state×quality. Read it to decide what to prune or fix.
- **`memory-graph [--format mermaid|dot]`** — serializes the `superseded_by` lineage to stdout (paste into mermaid.live or `dot`); no server.

## Automatic session capture (`session-capture` / SessionEnd hook)

The `SessionEnd` hook (`scripts/hooks/session-end.sh`) calls `session-capture`, which writes a work-handoff refinement from the files this agent locked this session plus the dirty git tree — so a session that ends mid-work leaves a pickup point automatically. It no-ops on a clean tree with no session locks, skips on `clear`, is fail-open, and opts out via `OCTOCODE_NO_SESSION_CAPTURE=1`.
