# Per-Tool Benchmark

One file per Octocode tool. Each proves a single tool works across **its full
schema** and in **real workflows** — this is the "does our own surface work"
lane (for us), before any head-to-head comparison in [`../compare/`](../compare/).

Current default CLI catalog: 15 tools. Gated/legacy docs remain for release checks that enable them: `ghListReleases`, `ghSearchDiscussions`, and deprecated `ghHistoryResearch`.

| Surface | Tools |
|---|---|
| GitHub | [ghSearchCode](ghSearchCode.md) · [ghSearchRepos](ghSearchRepos.md) · [ghSearchPullRequests](ghSearchPullRequests.md) · [ghSearchIssues](ghSearchIssues.md) · [ghSearchCommits](ghSearchCommits.md) · [ghListReleases](ghListReleases.md) *(env-gated: `ENABLE_RELEASES=1`)* · [ghSearchDiscussions](ghSearchDiscussions.md) *(env-gated: `ENABLE_DISCUSSIONS=1`)* · [ghGetFileContent](ghGetFileContent.md) · [ghViewRepoStructure](ghViewRepoStructure.md) · [ghCloneRepo](ghCloneRepo.md) |
| Local | [localSearchCode](localSearchCode.md) · [localFindFiles](localFindFiles.md) · [localFindDeadCode](localFindDeadCode.md) · [localGetFileContent](localGetFileContent.md) · [localViewStructure](localViewStructure.md) · [lspGetSemantics](lspGetSemantics.md) |
| Package | [npmSearch](npmSearch.md) |

## How to run

Every check is a CLI command. The CLI is the built entry:

```bash
CLI="node packages/octocode/out/octocode.js"
$CLI tools <name> --scheme --compact       # authoritative schema (params)
$CLI tools <name> --queries '<json>' --compact   # run the tool
```

Run from the **repo root**. Local tools need **absolute paths** (`"."` resolves
against cwd and can mismatch). GitHub/npm checks need `OCTOCODE_TOKEN` + network.

## What makes a check "smart"

Each tool file's checks are chosen to cover the tool, not to pad a list. Every
tool must be checked on all four:

1. **Happy path** — the primary use, correct data + anchors.
2. **The differentiating capability** — the thing this tool does that a dumb
   alternative can't (AST for `localSearchCode`, the two-step anchor for
   `lspGetSemantics`, `symbols` minify for content reads, `match:"path"` cheap
   existence for `ghSearchCode`, …). This is the most important check.
3. **Pagination / continuation** — page 2 preserves filters; `next.*` is runnable.
4. **Honest failure** — empty/404/unsupported returns an honest diagnostic, never
   a false proof of absence.

…plus **≥2 workflows** (multi-tool chains). Workflows are the point — a tool that
passes in isolation but doesn't hand off cleanly to the next tool fails the
benchmark.

## Output — record for every check

Each check run captures four numbers (the CLI writes to stdout; measure the run):

| Field | How |
|---|---|
| `time_ms` | wall-clock of the command |
| `tokens_out` | `ceil(stdout_chars / 4)` — the cost the model would pay to read it |
| `quality` | `1–5` (below) — is the output agent-ergonomic? |
| `score` | `2 / 1 / 0 / N/A` (below) — did it pass? |

Report them as a row per check: `{tool, check, exit, time_ms, tokens_out, quality, score, note}`.

## Judge — score each check in fresh context

A **judge** (a separate pass, blind to how the output was produced) scores each
check from the tool's stated expected output (the `→` clause). Deterministic
first: the presence of a field, a count, a path, or a `next.*` is code-checkable;
route only "is this output clean/ergonomic?" to a rubric.

| Score | Meaning |
|---:|---|
| `2` | Pass — correct data, no unexpected error, explicit pagination/continuation, enough anchors to continue without guessing. |
| `1` | Partial — useful, but one shape/hint/page field/anchor/timing is missing or ambiguous. |
| `0` | Fail — wrong/empty result, silent lossy mapping, schema drift, false proof of absence, or unexpected tool error. |
| `N/A` | Gated — auth, rate limit, clone disabled, LSP server missing, or provider down, with an **honest** diagnostic. |

| Quality | Meaning |
|---:|---|
| `5` | Exact anchors, concise output, valid evidence, runnable `next.*`, no stale hints. |
| `4` | Correct with minor verbosity/formatting friction. |
| `3` | Works, but a human must infer the next step. |
| `2` | Exits 0 but evidence/pagination/continuation is misleading. |
| `1` | Fails, uses stale schema, hides missing fields, or offers no repair path. |

A tool passes its benchmark when **every check scores 2** (or an honest `N/A`),
**quality ≥ 4**, and **every workflow reaches its proof**. `--scheme` is the
source of truth: if a check's params drift from `--scheme`, fix the check, not
the schema.
