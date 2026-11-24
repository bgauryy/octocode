Load when writing or running dependency rules over a saved graph (`octocode-graph check`), wiring them into CI with a baseline or SARIF, checking only what a pull request changed (`--since`), or explaining a violation's chain.

# Dependency rules

`octocode-graph check` evaluates a rules file over a saved graph and exits 1 when an `error` violation is new: one the baseline does not list. The vocabulary is [dependency-cruiser's](https://github.com/sverweij/dependency-cruiser/blob/main/doc/rules-reference.md) (`forbidden`, `allowed`, `required`, `reachable`, `circular`, `via`, `$1`), over every language the graph links and the packages it imports. Every violation carries the chain that proves it, each step with its import line and kinds.

```sh
octocode-graph build . --lsp off                    # once; later: octocode-graph refresh <result-dir>
octocode-graph check .                              # rules: .octocode/graph-rules.json at the graph root
octocode-graph check . --write-baseline             # accept today's violations; only new ones fail
octocode-graph check . --format sarif > graph.sarif # SARIF 2.1.0, each chain as codeFlows
octocode-graph check . --since origin/main          # only what the change introduced
```

## The rules file

`.octocode/graph-rules.json` at the graph root, or `--rules <file>`. The schema ships as `@octocodeai/octocode-graph/rules-schema` (`dist/scheme/rules.json`); the loader validates against it and names each problem's place (`forbidden[0].to.severity must be one of error, warn, info`).

```json
{
  "forbidden": [
    { "name": "no-node-in-browser", "severity": "error", "comment": "The viewer runs in a browser",
      "from": { "path": "^src/browser/" }, "to": { "path": "^pkg:node:", "reachable": true } },
    { "name": "no-cross-feature", "severity": "warn",
      "from": { "path": "^src/features/([^/]+)/" },
      "to": { "path": "^src/features/", "pathNot": "^src/features/$1/" } },
    { "name": "no-runtime-cycles", "severity": "warn", "from": {}, "to": { "circular": true } },
    { "name": "no-dead-files", "severity": "info", "from": { "path": "^src/index\\.ts$" },
      "to": { "path": "^src/", "pathNot": "\\.test\\.ts$", "reachable": false } }
  ],
  "required": [
    { "name": "controllers-extend-base", "severity": "error",
      "module": { "path": "-controller\\.ts$", "pathNot": "base-controller\\.ts$" },
      "to": { "path": "^src/controllers/base-controller\\.ts$" } }
  ]
}
```

| Field | Meaning |
|---|---|
| `path`, `pathNot` | A JavaScript regular expression (or a list: any item) over node ids: root-relative file paths, and `pkg:<name>` for packages (`pkg:node:fs` for a runtime module, `pkg:@scope/name` for an installed one) |
| `name`, `severity`, `comment` | The rule's id in reports, baselines and SARIF `ruleId`; `error` fails the check, `warn` (default) and `info` report only |
| `load` | Which imports count: `runtime` (default: no `import type`), `eager` (also no `import()`: what loads at startup), `all` (every import and compile-time dependency) |
| `to.reachable: true` | Every target reached from a from-module through any chain of imports; the violation's chain is the shortest one |
| `to.reachable: false` | Every target no from-module reaches: dead files, or packages nothing loads |
| `to.circular: true` | A dependency that leads back to its source; `via` (some module of the cycle matches) and `viaOnly` (every one does) take `path` and `pathNot`; `cycle` lists the ring |
| `dependencyTypes`, `dependencyTypesNot` (under `to`) | The import kinds as the graph records them (`static-import`, `type-import`, `dynamic-import`, `commonjs-require`, `rust-use`, `external-builtin`, `external-dev`, …): a dependency matches when one of its kinds is (not) listed. Direct and circular rules only; a reachable rule walks by `load` |
| `$1`…`$9`, `$0` | In the `to` side's `path` and `pathNot`, the groups the `from` side's `path` captured (`required`: the `module` side's `path`), matched as text |
| `allowed`, `allowedSeverity` | Each dependency must match one allowed rule (`from`, `to`, `load`, kinds), else it is `not-in-allowed` at `allowedSeverity` (default `warn`) |
| `required` | Each module matching `module` depends on a module matching `to.path` (`to.reachable: true`: through any chain) |

Rules check production edges, as queries do. A `path` that matches no file or package of the graph is an error with near names (exit 2), as `reach --forbid` refuses a misspelled target: a rule over nothing would hold forever. A pair joined only by calls (Go, Java, C#, Scala, Rust, C) has no import kind.

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

## Explain one violation

`query . "why <from> <to>"` lists the route through every distinct first hop of `<from>` that loads `<to>` at run time (`--eager`: at startup), each hop with its import line and kinds; `reach <entry>` rows carry the same `chain`. `reach <entry> --forbid <file|dir|pkg:name>` is the one-line form of a reachable rule. [Queries](query.md)
