# Wiki Files Map

Use this to see how the six `.octocode/` projection files relate to canonical SQLite and to each other. Label→file write mapping: `references/learning-loop.md`.

| File | Source | Relation |
|---|---|---|
| `MEMORY.md` | `query memories` | Superset: every ACTIVE memory, any label. |
| `GOTCHAS.md` | `query gotchas` | Subset of MEMORY: `label=GOTCHA` or has `failure_signature`. |
| `LEARN.md` | `query lessons` | Subset of MEMORY: `label` ∈ {DECISION, ARCHITECTURE, WORKFLOW, …}. |
| `BOOKMARKS.md` | memory `references[]` | Cross-cut of MEMORY: EXTERNAL refs only (URL/repo/doc), deduped. Local file refs are never listed here — read them as `Refs:`/`Missing refs:` lines inside MEMORY/GOTCHAS/LEARN, or via `query files` / `awareness/csv/files.csv`, since local paths vary per checkout and go stale. |
| `DEVELOPER_REVIEW.md` | `query developer-review` | Independent of MEMORY: feedback on the instructions themselves, written via `reflect record --fix-instructions`. |
| `AGENTS.md` | repo-profile counts + top rows | Digest of the other five: snapshot counts, top Gotchas/Lessons, pointers. Entry point agents read first, capped at 80 lines. |

State flow: `attend` / `memory record` / `reflect record` write SQLite (canonical, live) → `repo inject` projects it into these six files (capped leads, not proof).

Next agent's `attend` / `query` / `memory recall` reads live SQLite first; open a file here only when SQLite is unavailable or `attend.next` routes there.

Next: for publish/share mechanics load `references/repo-context-management.md`; for what each reflect label writes load `references/learning-loop.md`.
