Load when wiring `octocode-graph check` into CI: baseline, output formats, exit codes, or checking only what a change introduced (`--since`).

# Rules in CI

## Baseline and output

`--write-baseline` writes every current violation to `graph-rules.baseline.json` beside the rules file (`--baseline <file>` elsewhere): `{ "violations": [{ "rule", "from", "to" }] }`, sorted, for review in the diff. Later runs mark the listed ones `known`: reported in JSON, counted in the text, absent from SARIF, never failing. An entry no longer violated is listed as `fixed`; write the baseline again to drop it.

| Format | Carries |
|---|---|
| `text` (default) | One line per new violation (`severity rule from → to (kind)`), its chain `a.ts:3 [static-import] → b.ts:7 [external-builtin,static-import] → pkg:node:fs` and comment, then the counts |
| `json` | `failed`, `counts` (error, warn, info, known, fixed), every reported `violations` row (`rule`, `severity`, `type`, `kind`, `from`, `to`, `chain`, `cycle`, `known`), `fixed`, `since` |
| `sarif` | SARIF 2.1.0: a rule per name with its default level (`error`, `warning`, `note`), one result per new violation at the source module's import line, `codeFlows` with one step per hop, paths relative to the repository root (`%SRCROOT%`) |

Exit 0: no new error. 1: a new error. 2: the rules, baseline, revision or arguments cannot be used. MCP: `check_graph` with `result`, `rules`, `baseline`, `write_baseline`, `since` and `format` answers the same; a new error is an error result.

## Only what a change introduced

`check --since <rev>` reads git, never writes it (`rev-parse`, `merge-base`, `diff --name-only`, `ls-files --others`): the files changed since the merge base of `<rev>` and `HEAD`, plus uncommitted and untracked ones. It refreshes the saved graph's chunks that hold a change, then reports the violations the base graph does not have whose source changed or whose chain crosses a changed file; `since.preExisting` and `since.outside` count what it left out.

The base is the saved graph of the merge base: one an earlier `check --since` kept for that commit (`since-base.json` in the result directory), else the graph as it was before this refresh, else the graph its previous refresh replaced (`previous.json`), each only when the graph files it differs in are exactly those the diff lists. With none, `since.base` is null, the answer says so, and every violation is reported. So build or refresh the graph while the base is checked out, then check on the branch:

```yaml
# GitHub Actions, pull_request (not yet run in CI here)
- uses: actions/checkout@v4
  with: { ref: '${{ github.base_ref }}' }
- run: npx -y @octocodeai/octocode-graph build . --lsp off
- uses: actions/checkout@v4 # the pull request; clean: false keeps the base graph under .octocode/
  with: { fetch-depth: 0, clean: false }
- run: npx -y @octocodeai/octocode-graph check . --since 'origin/${{ github.base_ref }}' --format sarif > graph.sarif
- uses: github/codeql-action/upload-sarif@v3
  if: always()
  with: { sarif_file: graph.sarif, category: octocode-graph }
```

The upload needs `security-events: write`; `if: always()` uploads the log when the check fails the job.
