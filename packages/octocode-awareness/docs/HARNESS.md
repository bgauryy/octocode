# Octocode Awareness — Harness System Reference

The harness closes a self-improvement feedback loop: every session produces structured lessons, failure signatures, and refinement proposals that feed back into future sessions, letting agents observe their own patterns and propose targeted fixes under human approval.

---

## Agent Loop (Mental Model)

```
ATTEND → CLAIM → WORK → VERIFY → ENCODE → SLEEP
```

| Step | What happens | Key calls |
|------|-------------|-----------|
| **Attend** | Check workspace state, recall past lessons, read signals | `workspace_status`, `memory_recall`, `agent_signal action:list` |
| **Claim** | Declare intent, acquire file locks | `preFlightIntent()` / `file_lock type:lock` |
| **Work** | Edit files under lock; record durable findings mid-task | `insertMemory()` / `memory_record` |
| **Verify** | Run the declared test plan, record the result | `markVerified()` / `memory_verify` |
| **Encode** | Reflect on what happened; queue repo/harness fixes | `reflect()` / `memory_reflect` |
| **Sleep** | End session; digest if pruning is safe | `endSession()` / `memory_digest` |

---

## Improvement Loop (Full Flow)

```
┌─────────────────────────────────────────────────────────┐
│  START SESSION                                          │
│  registerAgent() ─── agents table: upserted            │
│  insertSession()  ─── sessions table: started_at set   │
└───────────────────────────────┬─────────────────────────┘
                                │
                                ▼
              preFlightIntent()                CLAIM
              ├── tasks: ACTIVE
              └── locks: per-file rows inserted

                                │
                          [agent edits files]
                                │
                                ▼
              ┌─────────────────────────────────────────┐
              │  PreToolUse hook: pre-edit.sh           │
              │  → hook-runner pre-edit                 │
              │  → preFlightIntent() (auto)             │
              │  harness-guard.sh: blocks if skill edit │
              │    without OCTOCODE_ALLOW_HARNESS_APPLY │
              └─────────────────────────────────────────┘
                                │
                          [files written]
                                │
                                ▼
              ┌─────────────────────────────────────────┐
              │  PostToolUse hook: post-edit.sh         │
              │  → hook-runner post-edit                │
              │  → releaseFileLock(status:'PENDING')    │
              │  ⚠️ edit_log NOT yet auto-populated     │
              │     (call insertEditLog() manually)     │
              └─────────────────────────────────────────┘
              tasks: ACTIVE → PENDING
              locks: deleted

                                │
                                ▼
              reflect()                        ENCODE
              ├── memories: EXPERIENCE inserted
              ├── harness_log: 'reflect' event
              └── refinements: optional repo-fix

                                │
                                ▼
              markVerified()                   VERIFY
              ├── tasks: PENDING → SUCCESS|FAILED
              └── task_log: VERIFIED event

                                │
                                ▼
              ┌─────────────────────────────────────────┐
              │  Stop/SubagentStop hook: stop-verify.sh │
              │  → hook-runner stop-verify              │
              │  → auditUnverified()                    │
              │  → blocks exit if PENDING count > 0     │
              │  (opt-out: OCTOCODE_NO_VERIFY_GATE=1)   │
              └─────────────────────────────────────────┘

                                │
                                ▼
              ┌─────────────────────────────────────────┐
              │  SessionEnd/PreCompact: session-end.sh │
              │  → hook-runner session-end              │
              │  → sessionCapture() (snapshot)          │
              └─────────────────────────────────────────┘
              sessions: ended_at set

                                │
                          [maintenance — on demand]
                                │
                                ▼
              mineWeakness()      ─── cluster failure_signatures
              mineDocStaleness()  ─── flag docs drifted from edit_log activity
              digest()            ─── prune stale data + export
              exportHarness()     ─── AGENTS.md / CLAUDE.md candidates

                                │
                human reviews + approves harness proposal
                                │
                         loop ──┘
```

---

## Hook Lifecycle

| Event | Hook script | hook-runner cmd | What it does |
|-------|-------------|-----------------|--------------|
| `PreToolUse` (Write/Edit) | `pre-edit.sh` | `pre-edit` | `preFlightIntent()` — claims task, inserts locks |
| `PreToolUse` (Write/Edit) | `harness-guard.sh` | `harness-guard` | Blocks if editing skill files without `OCTOCODE_ALLOW_HARNESS_APPLY=1` + non-main branch |
| `PostToolUse` (Write/Edit) | `post-edit.sh` | `post-edit` | `releaseFileLock(PENDING)` — releases locks |
| `Stop` / `SubagentStop` | `stop-verify.sh` | `stop-verify` | `auditUnverified()` — blocks exit on PENDING tasks |
| `SessionEnd` / `PreCompact` / host equivalent | `session-end.sh` | `session-end` | `sessionCapture()` — snapshot of session |
| `UserPromptSubmit` | `notify-deliver.sh` | `notify-deliver` | `notifyGet()` — injects smart briefing before each prompt |

**Edit audit gap**: `post-edit.sh` calls `releaseFileLock()` but does NOT call `insertEditLog()`. The `edit_log` table must be populated manually until the hook is wired.

---

## Harness Guard Decision Flow

```
PreToolUse (Write/Edit) fires on skill files?
           │
           ▼
  OCTOCODE_ALLOW_HARNESS_APPLY=1?
  ├── No  → block (exit 2) — "editing skill is gated"
  └── Yes → check git branch of skill root
            ├── main|master → block — "never on main"
            ├── detached HEAD|no branch
            │   └── OCTOCODE_HARNESS_BRANCH_OK=1?
            │       ├── No  → block
            │       └── Yes → allow
            └── feature branch → allow
```

---

## Installation Flow

When a new agent installs the skill:

```
1. node scripts/install.mjs [--check-only]
   ├── Check: Node >=22 (node:sqlite required)
   ├── Check: schema.mjs list (zod + schema resolvable)
   ├── If schema fails and not --check-only: npm install (scripts/package.json)
   └── Smoke tests:
       ├── schema.mjs example tell_memory  → validate output
      └── awareness.mjs maintenance self-test → DB + basic ops

2. octocode-awareness hooks install --host <claude|codex|cursor> [--project-dir <path>] [--global]
   ├── --dry-run   → show merged settings, don't write
   ├── hooks check  → report hook install status only
   ├── hooks remove → remove only our hooks
   └── writes host hook config to:
       Claude project/global: .claude/settings.json
       Codex project/global:  .codex/hooks.json
       Cursor project/global: .cursor/hooks.json
```

**Dependencies**: Node >=22, `zod` (local to `scripts/package.json`), SQLite built-in to Node.
**DB location**: `~/.octocode/memory/awareness.sqlite3` (override: `OCTOCODE_MEMORY_HOME`).

---

## DB Entities — Complete Reference

### `agents`
Registry of all agents that have touched the store. Updated on every tool call.

| Column | Type | Notes |
|--------|------|-------|
| `agent_id` | TEXT PK | e.g. `"cursor:abc123"`, `"pi:xyz"` |
| `agent_name` | TEXT | Display name; `''` if unknown |
| `workspace_path` | TEXT | Last-seen workspace |
| `context` | TEXT | `pi` · `cursor` · `claude-code` · etc. |
| `registered_at` | TEXT | First seen (ISO) |
| `last_seen_at` | TEXT | Updated every `registerAgent()` / `touchAgent()` |

**SQL module**: `sql/agents.ts`
**Key functions**: `registerAgent()`, `touchAgent()`, `resolveAgentName()`, `listAgents()`

---

### `sessions`
One session = one contiguous agent work period. Anchors `tasks`, `edit_log`, `harness_log`.

| Column | Type | Notes |
|--------|------|-------|
| `session_id` | TEXT PK | prefix `sess_` |
| `agent_id` | TEXT NOT NULL | Owning agent |
| `workspace_path` | TEXT | Resolved absolute path |
| `repo` | TEXT | GitHub repo slug `owner/repo` |
| `ref` | TEXT | Git branch / tag / SHA |
| `started_at` | TEXT NOT NULL | ISO timestamp |
| `ended_at` | TEXT | NULL = session still active |
| `summary` | TEXT | Set by `endSession()` |

**FK parent of**: `tasks.session_id`, `locks.session_id`, `edit_log.session_id`, `harness_log.session_id`
**SQL module**: `sql/sessions.ts`
**Key functions**: `insertSession()`, `getOrCreateSession()`, `endSession()`, `listSessions()`

---

### `memories`
Core knowledge store. Every `insertMemory()`, `reflect()`, and `memory_record` writes here.

| Column | Type | Notes |
|--------|------|-------|
| `memory_id` | TEXT PK | prefix `mem_` |
| `agent_id` | TEXT NOT NULL | Author |
| `task_context` | TEXT NOT NULL | What the agent was doing |
| `observation` | TEXT NOT NULL | The insight / finding |
| `importance` | INTEGER 1–10 | Salience weight |
| `state` | TEXT | `ACTIVE` · `SUPERSEDED` |
| `label` | TEXT | `EXPERIENCE` · `DECISION` · `ARCHITECTURE` · `GOTCHA` · `SECURITY` · `OVERRIDE` · `OTHER` |
| `superseded_by` | TEXT | Self-FK → `memories.memory_id` |
| `tags_json` | TEXT | JSON array |
| `workspace_path` | TEXT | Scope |
| `repo` | TEXT | Scope |
| `ref` | TEXT | Scope |
| `file_tree_fingerprint` | TEXT | Workspace snapshot hash |
| `failure_signature` | TEXT | `mechanism:X\|cause:Y\|surface:Z` for `mineWeakness()` |
| `novelty_score` | REAL | 0–1 Jaccard novelty vs. existing memories |
| `access_count` | INTEGER | Incremented on recall |
| `last_accessed_at` | TEXT | Updated on every `getMemory()` hit |
| `decay_half_life_days` | REAL | Per-label: DECISION/ARCH/SEC/GOTCHA=90d, EXPERIENCE=14d, default=30d |
| `valid_from` / `valid_to` | TEXT | Bi-temporal validity window |
| `expired_at` | TEXT | Set when valid_to passes |
| `embedding` | BLOB | Dense vector (optional, caller-provided) |
| `embedding_model` | TEXT | Model identifier for vector |
| `created_at` / `updated_at` | TEXT | |

**State machine**: `ACTIVE → SUPERSEDED` (via `superseded_by` + update)
**OVERRIDE label**: always surfaced in briefing regardless of importance floor or decay
**FK parent of**: `memory_refs.memory_id` (CASCADE), `harness_log.memory_id` (SET NULL)
**FTS index**: `memories_fts` covers `task_context`, `observation`, `tags`
**SQL module**: `sql/memory.ts`

---

### `memories_fts`
FTS5 virtual table for lexical search. BM25 column weights: `task_context=10`, `observation=7`, `tags=2`.

---

### `memory_refs`
Normalized reference index. Replaces the old `references_json` blob column.

| Column | Type | Notes |
|--------|------|-------|
| `memory_id` | TEXT | FK → `memories` ON DELETE CASCADE |
| `reference` | TEXT | URI, file path, repo:sha, tool ID |
| `kind` | TEXT | `url` · `file` · `dir` · `other` |
| `ordinal` | INTEGER | Insertion order |

**SQL module**: `sql/memory.ts`

---

### `tasks`
Pre-flight intent: agent declares files → acquires locks → releases → verify.

| Column | Type | Notes |
|--------|------|-------|
| `task_id` | TEXT PK | prefix `task_` |
| `agent_id` | TEXT NOT NULL | Claimant |
| `session_id` | TEXT | FK → `sessions` ON DELETE SET NULL |
| `rationale` | TEXT NOT NULL | Why the agent needs these files |
| `test_plan` | TEXT NOT NULL | Verification step the agent will run |
| `status` | TEXT | `ACTIVE` · `PENDING` · `SUCCESS` · `FAILED` |
| `workspace_path` | TEXT | Scope |
| `files_json` | TEXT | JSON array of absolute target paths |
| `created_at` / `updated_at` | TEXT | |

**State machine**:
```
ACTIVE ──(releaseFileLock)──► PENDING ──(markVerified SUCCESS)──► SUCCESS
                                       └──(markVerified FAILED)──► FAILED
```
- `ACTIVE`: locks held, work in progress
- `PENDING`: locks released, awaiting verification
- `SUCCESS` / `FAILED`: verified by agent

**FK parent of**: `locks.task_id` (CASCADE), `task_log.task_id` (SET NULL), `harness_log.task_id` (SET NULL)
**SQL module**: `sql/tasks.ts`

---

### `locks`
One row per file per task. Deleted on `releaseFileLock()`.

| Column | Type | Notes |
|--------|------|-------|
| `lock_id` | TEXT PK | prefix `lock_` |
| `file_path` | TEXT NOT NULL | Absolute path |
| `task_id` | TEXT NOT NULL | FK → `tasks` ON DELETE CASCADE |
| `agent_id` | TEXT NOT NULL | Lock holder |
| `session_id` | TEXT | Informational (not FK) |
| `lock_type` | TEXT | `EXCLUSIVE` · `SHARED` |
| `acquired_at` | TEXT | |
| `expires_at` | TEXT | NULL = no TTL; set = auto-evict at `evictExpiredLocks()` |

Lock TTL default: `MAX_LOCK_TTL_MS = 10min`. Enforced by `evictExpiredLocks()` — called on connect and in `pruneStale()`.
**SQL module**: `sql/tasks.ts`

---

### `task_log`
Immutable audit events for task lifecycle transitions.

| Column | Type | Notes |
|--------|------|-------|
| `event_id` | TEXT PK | prefix `evt_` |
| `task_id` | TEXT | FK → `tasks` ON DELETE SET NULL |
| `agent_id` | TEXT NOT NULL | Actor |
| `event_type` | TEXT | `VERIFIED` · `ABANDONED` |
| `message` | TEXT NOT NULL | Human-readable note |
| `created_at` | TEXT | |

**SQL module**: `sql/tasks.ts`

---

### `signals`
Agent-to-agent messages: claims, questions, handoffs, blockers, decisions.

| Column | Type | Notes |
|--------|------|-------|
| `signal_id` | TEXT PK | prefix `ntf_` |
| `workspace_path` | TEXT NOT NULL | Scope |
| `repo` / `ref` | TEXT | Scope |
| `from_agent` | TEXT NOT NULL | Sender |
| `to_agent` | TEXT | NULL = broadcast |
| `kind` | TEXT | `claim` · `handoff` · `question` · `reply` · `blocker` · `request` · `decision` · `fyi` |
| `subject` | TEXT NOT NULL | Short description |
| `body` | TEXT | Long-form content |
| `files_json` | TEXT | Related files |
| `refs_json` | TEXT | Related IDs or URLs |
| `thread_id` | TEXT NOT NULL | Groups replies together |
| `reply_to` | TEXT | Self-FK → `signals.signal_id` |
| `importance` | INTEGER 1–10 | |
| `status` | TEXT | `open` · `resolved` |
| `resolved_at` | TEXT | Set by `resolveNotification()` |
| `created_at` | TEXT | |

**State machine**: `open → resolved`
**FK parent of**: `signal_reads.signal_id` (CASCADE)
**SQL module**: `sql/signals.ts`

---

### `signal_reads`
Read-cursor per recipient. Tracks which signals each agent acknowledged.

| Column | Type | Notes |
|--------|------|-------|
| `signal_id` | TEXT | FK → `signals` ON DELETE CASCADE |
| `agent_id` | TEXT | Reader |
| `read_at` | TEXT | |

**SQL module**: `sql/signals.ts`

---

### `refinements`
Self-improvement proposals from the harness loop. `quality:handoff` = flagged for human review.

| Column | Type | Notes |
|--------|------|-------|
| `refinement_id` | TEXT PK | prefix `ref_` |
| `agent_id` | TEXT NOT NULL | |
| `workspace_path` | TEXT | Scope |
| `repo` / `ref` | TEXT | Scope |
| `files_json` | TEXT | Affected files |
| `reasoning` | TEXT NOT NULL | Why this refinement is needed |
| `remember` | TEXT NOT NULL | What to add to AGENTS.md / skill docs |
| `quality` | TEXT | `good` · `bad` · `handoff` |
| `state` | TEXT | `open` · `ongoing` · `done` |
| `created_at` / `updated_at` | TEXT | |

**SQL module**: `sql/refinements.ts`
**Key functions**: `insertRefinement()`, `getRefinements()`, `updateRefinement()`, `deleteRefinement()`

---

### `edit_log`
Audit trail of every file edit. Currently requires manual `insertEditLog()` calls — the `post-edit` hook releases locks but does **not** auto-populate this table yet.

| Column | Type | Notes |
|--------|------|-------|
| `edit_id` | TEXT PK | prefix `edit_` |
| `session_id` | TEXT | FK → `sessions` ON DELETE SET NULL |
| `task_id` | TEXT | FK → `tasks` ON DELETE SET NULL |
| `agent_id` | TEXT NOT NULL | Editor |
| `file_path` | TEXT NOT NULL | Absolute path |
| `operation` | TEXT | `create` · `update` · `delete` · `move` · `rename` |
| `old_file_path` | TEXT | Populated for move/rename |
| `lines_added` | INTEGER | |
| `lines_removed` | INTEGER | |
| `content_hash` | TEXT | SHA-256 of file content after edit |
| `workspace_path` | TEXT | |
| `created_at` | TEXT | |

**Insert**:
```typescript
import { insertEditLog, sha256Hex } from '@octocodeai/octocode-awareness';
insertEditLog(db, {
  agentId: 'cursor:abc123', sessionId: sess.session_id, taskId: task.task_id,
  filePath: '/workspace/src/foo.ts', operation: 'update',
  linesAdded: 12, linesRemoved: 3, contentHash: sha256Hex(newContent),
  workspacePath: '/workspace',
});
```
**Query**: `queryEditLog(db, { agentId, filePath, taskId, since, limit })`
**SQL module**: `sql/audit.ts`

---

### `harness_log`
Lifecycle events from the self-improvement harness. `reflect()` auto-inserts a `reflect` event; other event types require explicit calls.

| Column | Type | Notes |
|--------|------|-------|
| `harness_id` | TEXT PK | prefix `harness_` |
| `session_id` | TEXT | FK → `sessions` ON DELETE SET NULL |
| `agent_id` | TEXT NOT NULL | Actor |
| `event_type` | TEXT | `mine` · `propose` · `validate` · `apply` · `capture` · `reflect` |
| `payload_json` | TEXT | Event-specific JSON |
| `memory_id` | TEXT | FK → `memories` ON DELETE SET NULL |
| `task_id` | TEXT | FK → `tasks` ON DELETE SET NULL |
| `created_at` | TEXT | |

**Event type semantics**:
| Event | When to emit |
|-------|-------------|
| `reflect` | Auto-emitted by `reflect()` on every session end reflection |
| `mine` | Emit after running `mineWeakness()` — include top clusters in payload |
| `propose` | Emit when proposing a harness/AGENTS.md change to the user, or a doc-refresh via `proposeDocRefresh()` (payload `failure_signature: "doc-staleness"`) |
| `capture` | Emit when capturing a lesson that may become a harness proposal |
| `validate` | Emit when running regression/smoke tests on a harness change candidate |
| `apply` | Emit when applying an approved harness change to skill files |

**Insert**: `insertHarnessLog(db, { agentId, eventType, memoryId, taskId, sessionId, payload })`
**Query**: `queryHarnessLog(db, { agentId, eventType, limit })`
**SQL module**: `sql/audit.ts`

---

## Doc Staleness Detection (`src/docs.ts`)

Detects drift between a doc (`ARCHITECTURE.md`, wiki page, generated index) and the source it describes, using `edit_log` — no new tables, no generation pipeline.

| Function | What it does |
|----------|--------------|
| `mineDocStaleness(db, { targets, workspacePath?, minEditsSinceSync?, minLinesSinceSync? })` | Pure read. For each `{ docFile, sourceDirs }` target, compares edit_log activity under `sourceDirs` against `docFile`'s own last edit_log timestamp (or all-time, if the doc was never tracked). Returns `{ checked, stale_count, entries[] }`. |
| `proposeDocRefresh(db, entry, { agentId, sessionId?, workspacePath?, artifact? })` | Records a `harness_log` `propose` event (`failure_signature: "doc-staleness"`, `target_file`, `proposed_change`, `evidence`) for one stale entry. Returns the `harness_id`. |

Defaults: `minEditsSinceSync = 5`, `minLinesSinceSync = 50` — either threshold alone is enough to flag `stale: true`.

```sh
octocode-awareness docs staleness \
  --targets-json '[{"docFile":"packages/foo/ARCHITECTURE.md","sourceDirs":["packages/foo/src"]}]' \
  [--workspace <path>] [--min-edits <n>] [--min-lines <n>] [--propose --agent-id <id>]
```

This is deliberately narrow: it flags drift for docs you explicitly configure, it never generates or rewrites doc content, and `edit_log` must already be populated (see the edit audit gap above) for the signal to be meaningful.

---

## Complete Entity Relationship

```
agents (no FK children — resolved at display time by agent_id)
  │
  └── resolved from: tasks.agent_id, locks.agent_id, memories.agent_id,
      edit_log.agent_id, harness_log.agent_id, signal.from_agent, etc.

sessions ──< tasks ──< locks
    │            │
    │            └─< task_log
    │
    ├──< edit_log
    └──< harness_log ──► memories (SET NULL)
                         memories ──< memory_refs (CASCADE)
                         memories ──< memories_fts (virtual)
                         memories self-ref: superseded_by

signals ──< signal_reads (CASCADE)
signals self-ref: reply_to

refinements (standalone — no FK to sessions/tasks)
```

**FK cascade rules**:
- `ON DELETE CASCADE`: `locks ← tasks`, `signal_reads ← signals`, `memory_refs ← memories`
- `ON DELETE SET NULL`: `tasks.session_id`, `edit_log.session_id`, `harness_log.session_id`, `task_log.task_id`, `harness_log.memory_id`, `harness_log.task_id`

---

## SQL Organization (`src/sql/`)

All SQL constants are centralized — one file per entity group. Source modules import constants; no inline SQL strings.

| File | Tables covered | Status |
|------|---------------|--------|
| `sql/memory.ts` | `memories`, `memories_fts`, `memory_refs` | ⚠️ `memory.ts` not yet importing |
| `sql/tasks.ts` | `tasks`, `locks`, `task_log` | ⚠️ `intents.ts` not yet importing |
| `sql/sessions.ts` | `sessions` | ✅ used by `sessions.ts` |
| `sql/agents.ts` | `agents` | ✅ used by `agents.ts` |
| `sql/signals.ts` | `signals`, `signal_reads` | ✅ used by `notifications.ts` |
| `sql/audit.ts` | `edit_log`, `harness_log` | ✅ used by `audit.ts` |
| `sql/refinements.ts` | `refinements` | ✅ used by `refinements.ts` |
| `sql/index.ts` | barrel re-export | |

Dynamic WHERE fragments use `{DYNAMIC_WHERE}` or `{PLACEHOLDERS}` sentinels — replace at call site:
```typescript
const sql = LOCKS_SELECT_ACTIVE.replace('{DYNAMIC_WHERE}', clauses.join(' AND '));
```

---

## State Machines

### Memory
```
ACTIVE ──(superseded_by set)──► SUPERSEDED
```

### Task
```
ACTIVE ──(releaseFileLock)──► PENDING ──(markVerified SUCCESS)──► SUCCESS
                                       └──(markVerified FAILED)──► FAILED
```
Stale-active: ACTIVE with all locks expired → detected by `auditUnverified()` as `stale_active[]`.

### Signal
```
open ──(resolveNotification)──► resolved
```

### Harness Event Sequence
```
mine → propose → validate → apply
         ↕              ↕
      capture        reflect
```
`reflect` is the ground-level event (every session). `validate` and `apply` are Stage 3/4 — infrastructure exists, runner not yet wired.

---

## Useful Query Patterns

```sql
-- Files edited in a session
SELECT file_path, operation, lines_added, lines_removed
FROM edit_log WHERE session_id = ? ORDER BY created_at;

-- All tasks for an agent, with status
SELECT task_id, status, rationale, test_plan
FROM tasks WHERE agent_id = ? ORDER BY created_at DESC;

-- Active locks in a workspace
SELECT l.file_path, l.lock_type, t.agent_id, l.expires_at
FROM locks l JOIN tasks t ON t.task_id = l.task_id
WHERE t.workspace_path = ? AND t.status = 'ACTIVE'
  AND (l.expires_at IS NULL OR l.expires_at > datetime('now'));

-- Harness history for a session
SELECT event_type, payload_json, created_at
FROM harness_log WHERE session_id = ? ORDER BY created_at;

-- Recurring failure patterns
SELECT failure_signature, COUNT(*) AS freq, AVG(importance) AS avg_imp
FROM memories WHERE failure_signature IS NOT NULL AND state = 'ACTIVE'
GROUP BY failure_signature ORDER BY freq DESC LIMIT 20;

-- Most-edited files (trajectory view)
SELECT file_path, COUNT(*) AS edits
FROM edit_log GROUP BY file_path ORDER BY edits DESC LIMIT 20;

-- Open refinements with context
SELECT refinement_id, quality, state, reasoning, remember
FROM refinements WHERE state IN ('open','ongoing') ORDER BY updated_at DESC;

-- Signals unread by an agent
SELECT s.signal_id, s.kind, s.subject, s.from_agent
FROM signals s
LEFT JOIN signal_reads sr ON sr.signal_id = s.signal_id AND sr.agent_id = ?
WHERE s.status = 'open' AND sr.signal_id IS NULL
ORDER BY s.created_at DESC;
```

---

## Quick Reference

| Operation | Function | Table(s) |
|-----------|----------|---------|
| Register agent | `registerAgent()` | `agents` |
| Start session | `insertSession()` / `getOrCreateSession()` | `sessions` |
| Claim task + locks | `preFlightIntent()` | `tasks`, `locks`, `sessions` |
| Release locks | `releaseFileLock()` | `locks`, `tasks` (→ PENDING) |
| Store memory | `insertMemory()` | `memories`, `memories_fts`, `memory_refs` |
| Recall memory | `getMemory()` | `memories`, `memories_fts` |
| Record file edit | `insertEditLog()` | `edit_log` |
| Reflect | `reflect()` | `memories`, `harness_log`, `refinements` |
| Verify task | `markVerified()` | `tasks` (→ SUCCESS/FAILED), `task_log` |
| Audit unverified | `auditUnverified()` | `tasks`, `locks` |
| Mine failures | `mineWeakness()` | `memories` |
| Mine doc drift | `mineDocStaleness()` / `proposeDocRefresh()` | `edit_log` / `harness_log` |
| End session | `endSession()` | `sessions` |
| Prune + export | `digest()` / `exportHarness()` | all |
| Send signal | `insertNotification()` | `signals` |
| Read signals | `getNotifications()` | `signals`, `signal_reads` |
| Log harness event | `insertHarnessLog()` | `harness_log` |

---

## Indexes (Performance Reference)

| Index | Table | Columns |
|-------|-------|---------|
| `idx_memories_importance` | memories | importance |
| `idx_memories_state` | memories | state |
| `idx_memories_label` | memories | label |
| `idx_memories_failure_sig` | memories | failure_signature |
| `idx_memories_workspace_path` | memories | workspace_path |
| `idx_memories_scope` | memories | workspace_path, repo, ref |
| `idx_memories_valid` | memories | valid_from, valid_to |
| `idx_tasks_status` | tasks | status |
| `idx_tasks_agent_status` | tasks | agent_id, status |
| `idx_locks_file_path` | locks | file_path |
| `idx_locks_expires_at` | locks | expires_at |
| `idx_edit_log_session` | edit_log | session_id |
| `idx_edit_log_task` | edit_log | task_id |
| `idx_edit_log_file` | edit_log | file_path |
| `idx_edit_log_created_at` | edit_log | created_at |
| `idx_harness_log_session` | harness_log | session_id |
| `idx_harness_log_event_type` | harness_log | event_type |
| `idx_harness_log_memory` | harness_log | memory_id |
| `idx_signals_status` | signals | status |
| `idx_signals_to_agent` | signals | to_agent |
| `idx_signals_thread` | signals | thread_id |
| `idx_agents_last_seen` | agents | last_seen_at DESC |
