# LLM Wiki And Repo Context

Live SQLite is operational truth. `query` reads it; `repo inject` publishes bounded
workspace files when agents/humans need context without DB access.

## Locations

| Location | Role |
|---|---|
| `~/.octocode/memory/awareness.sqlite3` | Canonical live plans, tasks, work, locks, memory, signals, verification. |
| `<workspace>/.octocode/` | Generated projections plus authored `.octocode/plan/**` documents. |

Do not hand-edit generated projections. Correct source/DB state, then regenerate.
Plan narrative is authored; live task checklists are not.

## Live First

```bash
octocode-awareness attend --workspace "$PWD" --query "current task" --compact
octocode-awareness work list --workspace "$PWD" --compact
octocode-awareness query workboard --workspace "$PWD" --format table --limit 10
octocode-awareness query files --workspace "$PWD" --format table --limit 50
```

Use JSON for agents/APIs, table for terminals, CSV for scripts, Markdown for bounded
review, and HTML for humans who need search/filter/sort.

## Generate

```bash
octocode-awareness repo inject --workspace "$PWD" --mode local --compact
```

`local` is machine-local. `share` means the owner intends to review/commit the
projection; ignored output produces a warning, not a `.gitignore` mutation.

Generated surfaces include:

- `AGENTS.md`: small map and pointers;
- `MEMORY.md`, `GOTCHAS.md`, `LEARN.md`, `BOOKMARKS.md`;
- `DEVELOPER_REVIEW.md`;
- full sortable CSV and HTML views;
- `awareness/manifest.json` with generation scope/budgets;
- compact generated references.

Active run files, locks, signals, and tasks remain live-query concerns; do not dump
them into every Markdown projection.

## Size Policy

- Markdown indexes are capped and point to CSV/HTML/query for overflow.
- Compact attend/workboard cap peers, paths, bodies, evidence, and IDs with omitted
  counts.
- Briefing/peer delivery uses fingerprints so unchanged state is not re-injected.
- Generate after meaningful durable changes or when an explicitly requested snapshot
  is stale—not after every edit.

Check `query files` before trusting file/bookmark references. Generated timestamps do
not prove underlying files are current.

## Root Discovery

Root `AGENTS.md` should contain one short pointer to `.octocode/AGENTS.md`. Preserve
all existing instructions; never replace root guidance with the wiki.

## Editing And Sharing

- Generated files may include machine-local paths. Review before commit/share.
- Memories/signals/projections are leads; current user instructions/source/tests win.
- `repo inject` never edits `.gitignore`.
- `maintenance digest` does not regenerate or shrink existing Markdown; inject after
  approved cleanup when file readers need the update.

Projection behavior is separate from plan documents: `repo inject` preserves
`.octocode/plan/**`.
