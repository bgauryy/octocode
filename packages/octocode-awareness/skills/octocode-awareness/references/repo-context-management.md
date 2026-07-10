# Repo Context Publication

Use this when publishing, sharing, debugging, or linking workspace `.octocode/` context. For which live/durable/generated output to choose, read `references/output-routing.md`. Label→file map: `references/learning-loop.md`.

Canonical data lives in global `~/.octocode/memory/awareness.sqlite3`. `query <view>` is the live read API; `repo inject` publishes selected DB state as bounded Markdown, CSV, HTML, manifest, and references under the workspace.

Do not hand-edit generated files. Correct the DB/source, then regenerate.

## When To Inject

Run `repo inject` when a high-value gotcha/decision/lesson/handoff should be discoverable from files, a human requests an inspectable snapshot, or stale projection health could mislead the next agent.

Skip inject for routine edits, transient locks, or every signal. Active work should use `attend` and `query`.

```bash
octocode-awareness repo inject --workspace "$PWD" --out .octocode --mode local --compact
```

Relative output paths resolve under `--workspace`. Markdown is capped; overflow points to full CSV/HTML/query rows. Use `query files` before trusting old file/bookmark references.
The manifest stores a live-source revision. `attend` warns when SQLite would generate a different snapshot. Injection reports retired manifest-owned files as `orphan_candidates`; rerun with `--prune-orphans` only after review. Unknown files and `.octocode/plan/**` are never removed.

## Share Policy

`repo inject` never edits `.gitignore`.

- `--mode local`: personal/machine-local projection; default until sharing is intentional.
- `--mode share`: owner intentionally plans to review/commit projections.
- Share mode plus an ignored `.octocode/` produces a warning; the user decides policy.

Review machine-local absolute paths before committing any generated file.

## Root Discovery Pointer

Agents load root `AGENTS.md`, while inject writes only `.octocode/AGENTS.md`. After inject:

1. Read root `AGENTS.md`; stop if it already mentions `.octocode/AGENTS.md`.
2. If it is hand-authored, ask before appending unless the user already approved.
3. Append one short pointer; preserve all existing content.

```markdown
## Octocode Awareness
For shared-repo memory, locks, gotchas, and live context, start with `attend --compact`. Read `.octocode/AGENTS.md` only when live SQLite is unavailable, `attend.next` routes there, or projection history matters. SQLite is canonical; wiki files are leads — prefer live `attend` / `query` / `memory recall`. Create/learn via `memory record` / `reflect record`; run `repo inject` only when file readers need a fresh snapshot. Never hand-edit `.octocode/*.md`.
```

Wiki files: `AGENTS` · `GOTCHAS` · `LEARN` · `MEMORY` · `BOOKMARKS` · `DEVELOPER_REVIEW` (see learning-loop label map).

Publication closes when generated files match the intended DB revision/scope, manifest and orphan warnings are reviewed, root discovery works, and current source/tests still confirm the projected claims.
