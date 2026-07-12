# Memory Recall Workflow

Read this before planning, editing, recording, superseding, or trusting a remembered fact. Ranking and semantic behavior live in `references/memory-ranking.md`; output selection lives in `references/output-routing.md`.

## Recall

`memory recall` reads canonical SQLite rows, not `.octocode/MEMORY.md`. Run it when prior lessons may change the plan:

A successful explicit recall updates bounded popularity metadata, not evidence recency; startup `attend` opts out of that feedback.

```bash
octocode-awareness memory recall --query "<task>" --workspace "$PWD" --smart --compact
```

Useful filters:

- Applicability: `--workspace`, `--artifact`, `--repo`, `--ref`; add `--strict-scope` for exact-only or `--global-only` for unscoped rows.
- Kind: repeat `--label`, `--tag`, or lifecycle `--state`; default state is `ACTIVE`.
- Provenance: repeat `--file`, `--file-regex`, `--reference`, or broad `--regex`.
- Time/value: `--as-of`, `--min-importance`, `--limit`, `--sort`.
- Judgment: `--smart`, `--semantic`, `--explain`; explain adds score components and effective `applied_filters` after widening.

Use `schema command memory recall` when payload fields matter. CLI flags and schema property names can differ, such as `--file` versus stored arrays.

## Automatic Prompt-Time Lead

`format=hook` searches the bounded normal 50-candidate pool, then requires two meaningful
query-token matches. It emits at most one scoped `Memory lead — verify` or stays silent;
signals and `OVERRIDE` items remain independent. The prompt is transient: no access-count
update, memory row, or prompt text in delivery state; Pi clears empty, consumed, and shutdown state.

## Trust And Recording

A hit is a lead. Current user instructions, source, tests, and fresh command output win. Validate file-backed claims and inspect `missing_references`/`query files` before acting.

Record only a scoped, reusable, evidence-backed lesson, decision, gotcha, or source lead. Routine status belongs in tasks/signals/refinements, not memory.

If new evidence corrects an active row, use `memory record --supersedes <id>`; replacement history stays immutable.
For reversible cleanup, preview/apply `memory archive`; `memory restore` revives only archived rows, never rows carrying `superseded_by`.
Reserve `memory forget --dry-run` plus apply for reviewed irreversible deletion; keep raw IDs and scope narrow.

## Closure

- Zero results mean broaden vocabulary/filtering or use `--smart`; they do not prove absence.
- A low-confidence result with `judgment_required` needs more evidence before use.
- After using a memory, verify the claim in current context. An existing path is only a lead.
- When the claim changes, supersede/archive it; forget only after review. Regenerate projections only if file readers need the update.

Use recall to inform the plan; proof comes from current artifacts and checks.
