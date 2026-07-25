# Per-Tool Benchmark

One file per Octocode tool. Each proves a single tool works across **its full
schema** and in **real workflows** — this is the "does our own surface work"
lane (for us), before any head-to-head comparison in [`../compare/`](../compare/).

13 tools:

| Surface | Tools |
|---|---|
| GitHub | [ghSearchCode](ghSearchCode.md) · [ghSearchRepos](ghSearchRepos.md) · [ghHistoryResearch](ghHistoryResearch.md) · [ghGetFileContent](ghGetFileContent.md) · [ghViewRepoStructure](ghViewRepoStructure.md) · [ghCloneRepo](ghCloneRepo.md) |
| Local | [localSearchCode](localSearchCode.md) · [localFindFiles](localFindFiles.md) · [localGetFileContent](localGetFileContent.md) · [localViewStructure](localViewStructure.md) · [lspGetSemantics](lspGetSemantics.md) |
| Package | [npmSearch](npmSearch.md) |
| Search | [oqlSearch](oqlSearch.md) *(env-gated: `ENABLE_OQL=1`)* |

## How to run

Every check is a CLI command. The CLI is the built entry:

```bash
CLI="node packages/octocode/out/octocode.js"
$CLI tools <name> --scheme --compact       # authoritative schema (params)
$CLI tools <name> --queries '<json>' --compact   # run the tool
```

Run from the **repo root**. Local tools need **absolute paths** (`"."` resolves
against cwd and can mismatch). GitHub/npm checks need `OCTOCODE_TOKEN` + network.

## Scoring (same for every tool)

| Score | Meaning |
|---:|---|
| `2` | Pass — correct data, no unexpected error, explicit pagination/continuation, enough anchors to continue without guessing. |
| `1` | Partial — useful, but one shape/hint/page field/anchor/timing is missing or ambiguous. |
| `0` | Fail — wrong/empty result, silent lossy mapping, schema drift, false proof of absence, or unexpected tool error. |
| `N/A` | Gated — auth, rate limit, clone disabled, LSP server missing, or provider down, with an **honest** diagnostic. |

A tool passes its benchmark when **every check scores 2** (or an honest `N/A`)
and **every workflow reaches the stated proof**. `--scheme` is the source of
truth: if a check's params drift from `--scheme`, fix the check, not the schema.
