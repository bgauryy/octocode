# Repo Context Management

Use this when a task involves `.octocode/` generated repo context, repo-level AGENTS guidance, memory indexes, CSV exports, a human-readable awareness view, or the awareness "auto wiki" workflow.

## Model

The SQLite awareness DB is canonical. Files under `.octocode/` are generated projections for agents, scripts, and humans:

- `.octocode/AGENTS.md` - concise generated repo context for agents.
- `.octocode/MEMORY.md` - active memory index.
- `.octocode/GOTCHAS.md` - repo traps, failures, and failure signatures.
- `.octocode/LEARN.md` - decisions, architecture notes, workflows, and opportunities.
- `.octocode/awareness/csv/*.csv` - filterable/sortable data for scripts and agents.
- `.octocode/awareness/index.html` - static browser view.
- `.octocode/awareness/manifest.json` - generation metadata and share/local policy warnings.
- `.octocode/references/` - compact generated reference notes to avoid context bloat.

Do not hand-edit generated projections when a DB update or regeneration is the right fix.

The repo context projection is wiki-like by design:

- `query <view>` is the live read API for agents and scripts.
- `repo inject` is the publication step that turns selected DB state into repo-local Markdown, CSV, HTML, manifest, and reference files.
- `reflect record`, `memory record`, signals, locks, verification, and refinements all feed the same DB, so the generated docs can summarize work without storing raw chat logs.
- Generated files are leads, not proof. Agents must validate them against current files, tests, and command output.

## Commands

Prefer live DB reads when freshness matters:

```bash
octocode-awareness query all --workspace "$PWD" --format json --limit 20 --compact
octocode-awareness query gotchas --workspace "$PWD" --format table
octocode-awareness query files --workspace "$PWD" --format csv --out .octocode/awareness/csv/files.csv
```

Write a human HTML view through the query command:

```bash
octocode-awareness query all --workspace "$PWD" --format html --out .octocode/awareness/index.html
```

Regenerate repo projections:

```bash
octocode-awareness repo inject --workspace "$PWD" --out .octocode --mode local --compact
```

## Share Policy

`repo inject` never edits `.gitignore`.

- Use `--mode local` when `.octocode/` is personal or machine-local.
- Use `--mode share` when the repo owner intentionally wants to commit the generated projections.
- If `--mode share` is requested while `.octocode/` is ignored, the command reports a warning. The user decides whether to remove the ignore rule.

In this monorepo, keep `.octocode/` ignored unless the user explicitly changes that policy.

## Operating Rules

- Read workspace `AGENTS.md` first, then `.octocode/AGENTS.md` if it exists.
- Treat generated memories as leads. Verify current files and command output before relying on them.
- Record durable new facts with `memory record` or `reflect record`, then regenerate projections if the repo context should reflect them.
- Prefer `query <view>` for agent automation and ad hoc exports; use `query all --format html --out ...` for humans; prefer `repo inject` only when the repo projection should be created or refreshed.
- Keep self-improvement separate from publication: `reflect mine-weakness` and `reflect export-harness` can propose harness guidance, but a human-reviewed edit changes skills or repo docs.
