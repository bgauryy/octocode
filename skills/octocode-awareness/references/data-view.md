# Awareness Data Viewer

**Call this whenever the user asks to *show / view / see / browse / open* their awareness data** (memories, refinements, notifications, locks, intents, harness/verify status, "show awareness", "show my memory"). Do **not** hand-dump rows into chat — render the viewer and open it in the browser. Use `get-memory` (text) only for programmatic recall *mid-task*, never when the user wants to *look at* the data.

## One store, one view

Everything lives in the **single shared store** `~/.octocode/memory/awareness.sqlite3` (relocate with `OCTOCODE_MEMORY_HOME`). The viewer reads **every data table** from it and renders five sortable, filterable panels:

| Panel | Table(s) | Shows |
|-------|----------|-------|
| **Memories** | `agent_memories` | Reusable lessons — importance, state (ACTIVE/SUPERSEDED), agent, file, tags, and **⚠ mined-weakness** (`failure_signature`) for harness signals |
| **Refinements** | `refinements` | Per-repo work handoffs — state (open/ongoing/done), quality, repo/ref, reasoning, what to remember |
| **Notifications** | `notifications` | Agent-to-agent messages — kind, importance, from/to, subject+body, thread, repo |
| **Intents & Harness** | `agent_intents` + `intent_events` | Pre-flight intents + **verify-before-conclude** status: per-intent `status` and whether its declared test-plan was **✓ verified** (from a `VERIFIED` event) |
| **File Locks** | `file_locks` | Files currently claimed by agents — agent, file, lock type, acquired/expires |

The viewer covers **all** awareness data — self-awareness (memories + refinements), files-awareness (locks), peer messaging (notifications), and the self-harness lane (intents/verify + mined weaknesses).

## Two-file design

- `scripts/show-memories.py` — pulls all rows from the shared DB (read-only `file:…?mode=ro`), shapes them, and fills the template. Stdlib only.
- `scripts/show-memories.template.html` — the self-contained UI (dark theme, sortable columns, search box, per-row delete). The script injects the JSON payload into the single `__AWARENESS_DATA__` slot; HTML-significant chars are `\uXXXX`-escaped so a poisoned memory can't break out of the data block (stored-XSS guard).

## How to call

```bash
# Live server (auto-opens browser; per-row delete buttons work; Ctrl-C to stop)
python3 <skill_root>/scripts/show-memories.py

# Static snapshot to a file, then open it (delete buttons just print the CLI command)
python3 <skill_root>/scripts/show-memories.py --no-serve --out ~/.octocode/memory/awareness-view.html
# macOS: open <file> · Linux: xdg-open <file>

# Don't launch a browser (CI / headless / you'll open it yourself)
python3 <skill_root>/scripts/show-memories.py --no-serve --no-open --out <file>.html
```

Useful flags: `--memory-db <path>` / `--workspace-db <path>` (point at a different store, e.g. tests), `--port` (default 8787), `--host`/`--allow-remote` (binding a non-loopback host is refused unless `--allow-remote`, since delete endpoints would be network-exposed).

## Serve vs snapshot

- **serve** (default): localhost HTTP server; delete buttons call back into `awareness.py` (`forget` / `refine-delete` / `notify-prune`) through the canonical path (FTS + read-cursor cleanup). CSRF-guarded. Use when the user wants to **prune** as well as look.
- **snapshot** (`--no-serve`): one read-only `.html` file you can keep or share; deletes only print the CLI command. Use when the user just wants to **see** everything (no server left running).

## Notes

- Memories are global; refinements/notifications/intents/locks are scoped by columns (`repo`/`ref`, `workspace_path`) but live in the same file — the viewer shows them all regardless of cwd.
- Intents/locks panels are read-only in the UI (no delete button) — intents close via `release-file-lock`/`verify`; locks via `release-file-lock`.
- See `references/show-memories.md` when auditing older viewer notes; this doc supersedes the storage details there.
