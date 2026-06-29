# show-memories — the HTML viewer

Read this to inspect or audit awareness data visually instead of reading raw JSON. `scripts/show-memories.py` reads the **one shared store** and renders five sortable panels: memories, refinements, notifications, intents, and locks. Stdlib only — no dependencies.

## Run it

```bash
# Serve on localhost (working delete buttons) and open a browser:
python3 <skill_root>/scripts/show-memories.py
python3 <skill_root>/scripts/show-memories.py --port 8787 --no-open

# Point at a specific shared store / workspace scope:
python3 <skill_root>/scripts/show-memories.py --memory-db ~/.octocode/memory/awareness.sqlite3 --workspace /path/to/repo

# Static, read-only snapshot (no server; opens in a browser; delete buttons show the CLI command):
python3 <skill_root>/scripts/show-memories.py --no-serve --out awareness.html
```

Flags: `--memory-db`, `--workspace`, `--workspace-db` (override the refinement DB directly), `--host` (default `127.0.0.1`), `--port` (default `8787`), `--no-serve` + `--out`, `--no-open`.

## What you get

- **Memories panel** (global): importance (color-graded), state badge (`ACTIVE`/`SUPERSEDED`), agent, **file**, observation, tags, created. Sorted by importance by default.
- **Refinements panel** (workspace): state badge (`open`/`ongoing`/`done`), quality (`good`/`bad`), agent, repo, ref, **file**, reasoning (next-agent note), remember, updated. Sorted by most-recent.
- **Notifications, intents, and locks panels**: live repo messages, verification debt, and current/expired file claims from the same shared store.
- **Sort** any column by clicking its header (toggles asc/desc). **Filter** all rows with the search box. **Refresh** reloads from disk (server mode) without losing your place. The page renders rows from embedded JSON via JavaScript, so it needs a JS-capable browser — the `--no-serve` snapshot is a portable interactive file, not a grep-able/diff-able static table.
- **File correlation**: every row shows the ONE file it relates to (basename, full path on hover) or `— general` when it has none. Memory, refinement, and lock paths are all normalized to absolute form — but relative paths resolve against the writer's **current working directory**, so a file only lines up across panels/`status` when agents pass absolute paths or run from the same repo root (see `coordination-protocol.md`).

## Delete buttons

In **serve** mode the delete button calls back into `awareness.py` — memory delete runs `forget --memory-id` (so the FTS index is cleaned too), refinement delete runs `refine-delete --refinement-id`. The endpoints (`POST /api/delete-memory`, `POST /api/delete-refinement`) are bound to localhost only. In **static** mode there is no server, so the button instead surfaces the exact CLI command to run.

## Security notes

- The server binds `127.0.0.1` by default — do not expose it; it can delete records. Override `--host` only on a trusted machine.
- The viewer is read-mostly: it never edits content, only deletes whole records through the canonical CLI path.
- The viewer honors the same store locations as the CLI, so what you see is exactly what agents recall.
