# Session Artifacts

Every file that Octocode writes during a session — plan pages, screenshots,
compaction snapshots, error logs, and more — lands in one **session artifact tree**
instead of scattered across OS temporary directories and various workspace
subdirectories.

---

## Where your files live

All session outputs are written under:

```
$OCTOCODE_HOME/extension/workspaces/<workspace-key>/sessions/<session-key>/
```

Extension-private SQLite state for MCP and skill overrides, catalog metadata, and
worker lifecycle records is stored at:

```
$OCTOCODE_HOME/extension/state/extension.sqlite3
```

This extension-owned database is separate from shared Awareness coordination and
memory state. Plans, tasks, interactions, and memories continue to use the canonical
shared database opened by `openAwareness` under `$OCTOCODE_HOME/agent`; they are an
explicit exception to the extension-private storage root and are not copied or forked.
With `storage.mode=memory`, the extension-private SQLite database is not opened.

The `session-key` is derived from the session ID (from the Pi session manager)
combined with a SHA-256 fingerprint of the session + workspace, so:

- **Different sessions** in the same workspace get different keys.
- **Same session** always resolves to the same directory, even after restart.
- The key format is: `<slug-of-session-id>-<12-char-hex-hash>` (for example, `my-session-a3f4b9c12d01`).

---

## Output map

| What | Path inside `<session-key>/` | Written by |
|------|------------------------------|------------|
| Plan page (HTML) | `plan/plan.html` | `plan` tool |
| Plan page (Markdown) | `plan/plan.md` | `plan` tool |
| Plan state snapshot | `plan/state.json` | `plan` tool |
| Plan branch snapshots | `plan/branches/*.json` | `plan` tool |
| Browser screenshots | `browser/screenshots/*.png` | `chromeDebug` tool |
| Chrome session metadata | `browser/port-<N>/session.json` | `chromeDebug` tool |
| Chrome CDP event log | `browser/port-<N>/cdp-events.jsonl` | `chromeDebug` (debug mode) |
| Compaction snapshot | `compaction/<timestamp>-<label>.md` | Compaction hook |
| Latest compaction snapshot | `compaction/latest.md` | Compaction hook |
| Checkpoint store pointer | `checkpoint-ref.json` | Checkpoint engine |
| Error / warning log | `logs/error.txt` | Internal error handler |
| Fallback images (PNGs) | `images/<name>-<timestamp>.png` | `media` |
| Export HTML reference | `export/latest-ref.json` | `/octocode-export` command |
| Session manifest | `manifest.json` | All producers (auto-updated) |

Plan state writes use V4. Unlike V3, V4 preserves lifecycle and review metadata when no
execution steps exist and records an explicit `cleared` tombstone. V3 snapshots remain
readable. Resume and tree navigation restore the selected branch snapshot; a fork demotes
inherited executing/verifying/blocked work to accepted (or draft), removes shared task
mappings, and requires a new Start and claim in the fork.

---

## The manifest

Every time a file is written, its relative path is recorded in `manifest.json`
at the session root. You can open it any time to see exactly what the current
session has produced:

```json
// $OCTOCODE_HOME/extension/workspaces/<workspace-key>/sessions/<session-key>/manifest.json
{
  "version": 1,
  "sessionKey": "my-session-a3f4b9c12d01",
  "workspace": "/Users/you/myproject",
  "createdAt": "2026-08-24T10:00:00.000Z",
  "updatedAt": "2026-08-24T11:30:42.000Z",
  "producers": {
    "plan": {
      "firstSeenAt": "2026-08-24T10:01:00.000Z",
      "lastSeenAt": "2026-08-24T11:30:42.000Z",
      "paths": ["plan/plan.html", "plan/plan.md", "plan/state.json"]
    },
    "browser": {
      "firstSeenAt": "2026-08-24T10:05:00.000Z",
      "lastSeenAt": "2026-08-24T10:05:02.000Z",
      "paths": ["browser/screenshots/screenshot-1234567890.png"]
    },
    "log": {
      "paths": ["logs/error.txt"]
    }
  }
}
```

---

## View the plan page

The plan HTML file (`plan/plan.html`) auto-refreshes every 10 seconds. You can
open it in a browser directly:

```sh
open "$(ls -dt "$OCTOCODE_HOME"/extension/workspaces/<workspace-key>/sessions/*/plan/plan.html | head -1)"
```

Or use the `localServer` tool inside Octocode to serve it:

```
/octocode-plan
```

The plan page renders:
- Current steps with status icons (todo / doing / done)
- The linked RFC document (if any)
- Decision log entries
- A dependency diagram (Mermaid)

---

## Error logs

The error log captures extension-visible problems: tool failures (`isError: true`),
hook exceptions, and provider HTTP errors ≥ 400.

```sh
# Find the current session's error log
ls -t "$OCTOCODE_HOME"/extension/workspaces/<workspace-key>/sessions/*/logs/error.txt | head -1 | xargs cat
```

Each entry includes: timestamp, uptime, source, cwd, model, severity, duration
(for tool/provider failures), redacted details, and stack trace for errors.

> **Note:** Secret-like fields (`authorization`, `token`, `cookie`, `secret`,
> `password`, API keys) are **redacted** before writing.

---

## Compaction snapshots

After Pi successfully compacts the conversation context, Octocode writes a
Markdown recovery checkpoint to:

- **Timestamped snapshot:** `compaction/<timestamp>-<label>.md` — never overwritten; one file per compaction event.
- **Latest pointer:** `compaction/latest.md` — always the most recent snapshot.

The checkpoint includes a secret-redacted copy of Pi's summary, read and modified file pointers, and the
active plan with its RFC and step paths. It is a recovery hint. Reopen the current
plan and referenced docs before resuming; current sources override stale snapshot
text.

### Compaction and smart-resume flow

1. After Pi finishes its post-run checks and emits the idle `agent_settled`
   boundary, Octocode requests compaction through Pi's public API when context
   usage reaches 80%. Pi selects the history boundary
   and summarizes it. On an
   overflow split turn only, Octocode can supply a bounded deterministic fallback
   that preserves resume instructions, plan and doc pointers, file lists, and
   Pi's split-turn marker.
2. After a successful `session_compact`, Octocode clears stale file-read state,
   writes the checkpoint, stages a digest-bound rehydration ledger, and shows one
   `context compacted — checkpoint ready` card.
   A failed compaction releases the threshold request guard without writing a
   success checkpoint, so a later settled boundary can retry safely.
3. If `willRetry` is true, Pi retries the interrupted agent turn. Octocode does
   not start a competing continuation. Manual and threshold compactions likewise
   do not create an extra turn.
4. Smart resume compares the saved ledger with the live plan and registered
   context owners. It restores only matching, unexpired, explicitly eligible
   segments from
   their current owners; saved bodies never override newer plan or doc state. The
   aggregate projection is capped at 8,000 estimated tokens. Generic tool results,
   prior user requests, and selected skill bodies are not automatically replayed.
   The ledger expires after 24 hours.
5. The prompt resumes only authorized unfinished work. A complete request, an
   approval gate, or a user wait state remains stopped.

Large provider-visible tool results use the same artifact root. Omitted text is
stored under `tool-results/*.txt`; excess images are stored as binary files with
a `tool-results/*-images.json` manifest containing their MIME types, sizes, and
resolved paths.

If ledger staging fails, compaction still succeeds, and the checkpoint card still
appears. Continue from Pi's summary and the current active plan.

---

## Checkpoint pointer

The shadow-git checkpoint store intentionally lives **outside** the working repository to
avoid polluting version control:

```
$OCTOCODE_HOME/extension/checkpoints/<cwd-hash>/
```

The session artifact tree contains a lightweight JSON pointer at `checkpoint-ref.json`:

```json
{ "storeDir": "/Users/you/.octocode/extension/checkpoints/abc123/", "cwd": "/Users/you/myproject" }
```

This lets the session manifest track *that* checkpointing happened, without
moving the actual git objects.

---

## Fallback behavior

Every route has a safe fallback for situations where the workspace doesn't yet
exist or the session context is absent (for example, during early startup):

| Producer | Fallback path |
|----------|---------------|
| `plan` | `$OCTOCODE_HOME/extension/tmp/plan/<scope-hash>/` |
| `browser` | `$OCTOCODE_HOME/extension/workspaces/<workspace-key>/` |
| `compaction` | No separate fallback write; the Pi transcript remains authoritative if the artifact write fails |
| `log` | `$OCTOCODE_HOME/extension/workspaces/<workspace-key>/logs/error.txt` |
| `image` | `$OCTOCODE_HOME/extension/tmp/images/<session-id>/` |
| Shell and dynamic-tool spill files | `$OCTOCODE_HOME/extension/tmp/` |
| MCP package cache | `$OCTOCODE_HOME/extension/cache/mcp-npx/` |

Fallback writes are never registered in the session manifest, so the manifest
reflects only session-scoped artifacts. Extension-owned fallback storage never
leaves `$OCTOCODE_HOME/extension/`.

---

## File security

- Directories: created with mode `0700` (owner read/write/execute only).
- Files: written with mode `0600` (owner read/write only).
- All writes use an atomic temp-file + rename pattern — no partial reads.
- Symlink escape is checked at each path boundary (traversal cannot escape the session root).
- The manifest is protected by a `O_EXCL` lock file during every update.

---

## Clean up

Session artifact trees accumulate over time. Each tree is small (a few KB to a
few MB depending on screenshot count). You can safely delete old session trees:

```sh
# List all session trees, sorted by age
ls -lt "$OCTOCODE_HOME"/extension/workspaces/<workspace-key>/sessions/

# Remove trees older than 30 days
find "$OCTOCODE_HOME"/extension/workspaces/<workspace-key>/sessions -maxdepth 1 -type d -mtime +30 -print

# After reviewing the printed paths, remove only the exact session directories you selected.
```

> Session artifacts are global and never need a repository `.gitignore` entry.

---

## Internals (developer reference)

The session artifact system is implemented in:

| File | Role |
|------|------|
| `src/tools/session-artifacts.ts` | Core API: `createSessionArtifactContext`, `resolveSessionIdentity`, CAS projection, branch snapshots |
| `src/tools/active-plan.ts` | Exports `artifactContextForScope(scope)` — bridges plan scope → session identity |
| `src/tools/plan-html.ts` | Writes `plan/plan.html` + `plan/plan.md` via the artifact context |
| `src/chrome-debug.ts` | `getSessionDir(cwd, port, sessionKey?)` and `getScreenshotDir(cwd?, sessionKey?)` |
| `src/tools/chrome-debug-tool.ts` | Resolves `sessionKey` from `resolveSessionIdentity` and passes to `connectToChrome` |
| `src/tools/compaction-artifacts.ts` | `writeCompactionArtifact(details, session?, cwd?)` — session path when cwd+session provided |
| `src/tools/compaction-hooks.ts` | Passes `ctx.cwd` to `writeCompactionArtifact` |
| `src/tools/export-command.ts` | Registers `export/latest-ref.json` after writing the branded HTML export |
| `src/tools/create-image-tool.ts` | `persistFallbackPng` — writes to `images/` in the session tree, with an extension temp fallback |
| `src/extension-paths.ts` | Canonical `$OCTOCODE_HOME/extension` root and workspace, temp, and cache projections |
| `src/index.ts` | `getInternalErrorLogPath` returns `logs/error.txt` inside session tree; checkpoint ref registered on engine init |

### Add a new producer

1. Add your producer name to `SessionArtifactProducer` in `session-artifacts.ts`.
2. Import `createSessionArtifactContext` (or `resolveSessionIdentity` for path-only) in your tool.
3. Call `ctx.writeText` / `ctx.writeJson` for the actual content.
4. Call `ctx.registerProducer('your-slot', 'relative/path.ext')` after the write.
5. Provide a fallback write path for when the session context is unavailable.
6. Add a test case to `tests/session-artifact-wiring.test.ts`.
