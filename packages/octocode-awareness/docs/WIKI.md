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

## Read And Write Map

| Intent | Surface | Canonical effect |
|---|---|---|
| Inspect current work | `attend`, targeted `query`, `work list|show`, `verify audit`, `workspace status` | Reads live state; does not close work or prune rows. |
| Plan and coordinate | `plan`, `task`, `work`, `lock`, `signal`, `refinement`, `verify mark` | Writes operational rows to SQLite. |
| Learn across runs | `memory record`, `reflect record`, `memory record --supersedes` | Writes durable knowledge or immutable replacement history. |
| Reversible cleanup | `memory archive --dry-run`, then archive/restore | Hides archived rows from active recall; restore never revives replacement history. |
| Read learned context | `memory recall` | Reads memories and updates access metadata for popularity; reads never refresh evidence age. |
| Mark communication read | `signal list --mark-read`, `signal ack` | Writes recipient/read state. Plain `signal list` does not. |
| Clean stale state | digest/prune/forget/delete commands | Mutates only after an explicit reviewed call; dry-run first. Digest preserves open/ongoing handoffs and reports signal/reference pressure without deleting those rows. |
| Author plan reasoning | `.octocode/plan/<timestamp-name>/`, then `plan doc` | Writes narrative; task status stays in SQLite. |
| Publish a file snapshot | `repo inject` | Regenerates bounded projections; never becomes operational truth. |

Hook briefing may update delivery fingerprints so unchanged context stays silent.
That bookkeeping does not acknowledge signals or prove work complete.

## Live First

```bash
octocode-awareness attend --workspace "$PWD" --query "current task" --compact
octocode-awareness work list --workspace "$PWD" --compact
octocode-awareness query workboard --workspace "$PWD" --format table --limit 10
octocode-awareness query files --workspace "$PWD" --format table --limit 50
```

Use JSON for agents/APIs, table for terminals, CSV for scripts, Markdown for bounded
review, and HTML for humans who need search/filter/sort. Query results report
`is_partial`, `total`, `omitted_count`, and `continuation`; a `null` total means a
safety probe found more rows but did not scan far enough to claim an exact total.

## Generate

```bash
octocode-awareness repo inject --workspace "$PWD" --mode local --compact
# If orphan_candidates are correct after review:
octocode-awareness repo inject --workspace "$PWD" --mode local --prune-orphans --compact
```

`local` is machine-local. `share` means the owner intends to review/commit the
projection; ignored output produces a warning, not a `.gitignore` mutation. Share
rows omit signal bodies and redact absolute paths. CSV prefixes spreadsheet-formula
cells, and each projection file is atomically replaced after its temporary file is
complete.

Generated surfaces include:

- `AGENTS.md`: small map and pointers;
- `MEMORY.md`, `GOTCHAS.md`, `LEARN.md`, `BOOKMARKS.md`;
- `DEVELOPER_REVIEW.md`;
- sortable CSV and HTML views, 50 rows by default and at most 500 per section;
- `awareness/manifest.json` with generation scope, live-source revision, completeness, budgets, and retired-file cleanup receipts;
- `references/repo-map.md`; invariant command/testing/architecture prose stays in the skill/docs instead of being duplicated.

Active run files, locks, signals, and tasks remain live-query concerns; do not dump
them into every Markdown projection.

## Size Policy

- Markdown indexes are capped and point to CSV/HTML/query for overflow.
- Compact attend/workboard cap peers, paths, bodies, evidence, and IDs with omitted
  counts.
- Briefing/peer delivery uses fingerprints so unchanged state is not re-injected.
- Generate after meaningful durable changes or when an explicitly requested snapshot
  is stale—not after every edit.

`AGENTS.md` contains no memory observations; agents fetch one targeted live row when
needed. Check `query files` before trusting references. Path existence is still only a
lead, not content verification. A complete manifest may compare its bounded source
revision with live SQLite. A partial manifest skips that expensive comparison and
routes agents to live SQLite for omitted rows.

## Root Discovery

Root `AGENTS.md` should contain one short pointer to `.octocode/AGENTS.md`. Preserve
all existing instructions; never replace root guidance with the wiki. Ask before
editing root instructions unless the user already authorized that change.

## Editing And Sharing

- Local generated files may include machine-local paths. Share mode redacts known
  absolute paths, signal bodies, and recognized secret patterns; it is not complete
  DLP. Never store secrets, and review share output before commit.
- Memories/signals/projections are leads; current user instructions/source/tests win.
- `repo inject` never edits `.gitignore`.
- `maintenance digest` does not regenerate or shrink existing Markdown; inject after
  approved cleanup when file readers need the update.

Projection behavior is separate from plan documents: `repo inject` preserves
`.octocode/plan/**`.
