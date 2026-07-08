# Repo Context Management

Use this when a task involves workspace `.octocode/` generated repo context, repo-level AGENTS guidance, memory indexes, CSV exports, a human-readable awareness view, or the awareness LLM Wiki workflow.

## Model

The SQLite awareness DB in the global Octocode home is canonical. Files under the workspace `.octocode/` folder are generated projections for agents, scripts, and humans:

- global home: `~/.octocode/` stores config and durable Awareness data such as `memory/awareness.sqlite3`.
- workspace projection: `<repo>/.octocode/` stores generated repo context and memories-about-this-repo as Markdown, CSV, HTML, manifest, and references.

`.octocode/MEMORY.md` is not where memories live. It is a generated readable index of selected `memories` rows from the global DB.

Generated files include:

- `.octocode/AGENTS.md` - concise generated repo context for agents.
- `.octocode/MEMORY.md` - active memory index.
- `.octocode/GOTCHAS.md` - repo traps, failures, and failure signatures.
- `.octocode/LEARN.md` - decisions, architecture notes, workflows, and opportunities.
- `.octocode/BOOKMARKS.md` - learnable resource leads from memory references: URLs, repos, file paths, docs, papers, skills, and other URIs.
- `.octocode/awareness/csv/*.csv` - filterable/sortable data for scripts and agents.
- `.octocode/awareness/index.html` - static browser view.
- `.octocode/awareness/manifest.json` - generation metadata and share/local policy warnings.
- `.octocode/references/` - compact generated reference notes to avoid context bloat.

Do not hand-edit generated projections when a DB update or regeneration is the right fix.

The repo context projection is wiki-like by design:

- `query <view>` is the live read API for agents and scripts.
- `repo inject` is the publication step that turns selected DB state into repo-local Markdown, CSV, HTML, manifest, and reference files.
- `BOOKMARKS.md` is the resource index projection. Add learnable URLs, repo paths, file paths, papers, skills, and other URIs as memory references, then regenerate.
- Markdown files are capped readable projections, not unlimited storage. When rows exceed the projection budget, `repo inject` omits overflow rows and points agents toward CSV, HTML, or query views for the full sortable/filterable data.
- `reflect record`, `memory record`, signals, locks, verification, and refinements all feed the same DB, so the generated docs can summarize work without storing raw chat logs.
- Generated files are leads, not proof. Agents must validate them against current files, tests, and command output.

Use the projection as a smart wiki, not as a write target. Query live data before/during work; regenerate `.octocode/` after recording important gotchas, decisions, handoffs, or lessons that future agents should see without querying SQLite.

## Commands

Prefer live DB reads when freshness matters:

```bash
octocode-awareness query all --workspace "$PWD" --format json --limit 20 --compact
octocode-awareness query gotchas --workspace "$PWD" --format table
octocode-awareness query files --workspace "$PWD" --format csv --out .octocode/awareness/csv/files.csv
```

Relative `.octocode/...` output paths are resolved against `--workspace`, not the caller's process cwd. Use absolute `--out` only when intentionally writing somewhere else.

Write a human HTML view through the query command:

```bash
octocode-awareness query all --workspace "$PWD" --format html --out .octocode/awareness/index.html
```

Regenerate repo projections:

```bash
octocode-awareness repo inject --workspace "$PWD" --out .octocode --mode local --compact
```

Here `--out .octocode` means `<workspace>/.octocode/` when the command runs from the repo root.

## Smart Wiki Updates

Refresh the wiki when one of these changes would help the next agent:

- a high-importance memory, gotcha, decision, or architecture note was recorded,
- a handoff/refinement should be visible from files rather than only the DB,
- a repeated failure was reflected and should become repo guidance,
- humans asked for an inspectable snapshot, CSV, or HTML view,
- stale generated context could mislead an agent.

Do not refresh the projection for routine edits, transient locks, or every signal. `query <view>` is cheaper and fresher for active work; `repo inject` is the publication step.

## Share Policy

`repo inject` never edits `.gitignore`.

- Use `--mode local` when the workspace `.octocode/` projection is personal or machine-local.
- Use `--mode share` when the repo owner intentionally wants to commit the generated projections.
- If `--mode share` is requested while `.octocode/` is ignored, the command reports a warning. The user decides whether to remove the ignore rule.

In this monorepo, keep workspace `.octocode/` ignored unless the user explicitly changes that policy.

## Operating Rules

- Read workspace `AGENTS.md` first, then `<repo>/.octocode/AGENTS.md` if it exists.
- After `repo inject` creates or refreshes `.octocode/AGENTS.md`, ensure the workspace root `AGENTS.md` has a short pointer to it. Agents load root `AGENTS.md` by default; without the pointer they miss the awareness map.
- Treat generated memories as leads. Verify current files and command output before relying on them.
- Record durable new facts with `memory record` or `reflect record`, then regenerate projections if the repo context should reflect them.
- Prefer `query <view>` for agent automation and ad hoc exports; use `query all --format html --out ...` for humans; prefer `repo inject` only when the repo projection should be created or refreshed.
- Keep self-improvement separate from publication: `reflect mine-weakness` and `reflect export-harness` can propose harness guidance, but a human-reviewed edit changes skills or repo docs.
- If the projection reveals a repeated workflow gap, use `octocode-skills` or `npx octocode skill ...` to update/install/create the relevant skill after user approval.

## Root AGENTS.md Pointer (agent action)

`repo inject` writes `.octocode/AGENTS.md` only. It does not edit root `AGENTS.md`. Agents must do the discovery link:

1. After inject (or when `.octocode/AGENTS.md` exists and root has no pointer), open workspace root `AGENTS.md`.
2. If it already mentions `.octocode/AGENTS.md`, stop.
3. Otherwise append once (create the file if missing). Leave existing root content unchanged; keep gotchas/lessons/wiki content in `.octocode/` only.

```markdown
## Octocode Awareness
For shared-repo memory, locks, gotchas, and live context, read `.octocode/AGENTS.md` (generated by `octocode-awareness repo inject`). Prefer `attend` / `query` when freshness matters.
```

Gate: if root `AGENTS.md` is already a hand-authored harness file, ask the user before appending unless they already asked for this pointer.
