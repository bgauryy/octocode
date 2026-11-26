Load when writing or running dependency rules over a saved graph (`octocode-graph check`), or explaining a violation's chain.

# Dependency rules

`octocode-graph check` evaluates a rules file over a saved graph and exits 1 when an `error` violation is new: one the baseline does not list. The vocabulary is [dependency-cruiser's](https://github.com/sverweij/dependency-cruiser/blob/main/doc/rules-reference.md) (`forbidden`, `allowed`, `required`, `reachable`, `circular`, `via`, `$1`), over every language the graph links and the packages it imports. Every violation carries the chain that proves it, each step with its import line and kinds.

```sh
octocode-graph build . --lsp off                    # once; later: octocode-graph refresh <result-dir>
octocode-graph check .                              # rules: .octocode/graph-rules.json at the graph root
octocode-graph check . --write-baseline             # accept today's violations; only new ones fail
octocode-graph check . --format sarif > graph.sarif # SARIF 2.1.0, each chain as codeFlows
octocode-graph check . --since origin/main          # only what the change introduced
```

Every field of a rule: [rule fields](rule-fields.md). Baseline, output formats, exit codes and `--since` in CI: [rules in CI](rules-ci.md).

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

## Explain one violation

`query . "why <from> <to>"` lists every route by which `<from>` loads `<to>`, each hop with its import line and kinds; `reach <entry>` rows carry the same `chain`. `reach <entry> --forbid <file|dir|pkg:name>` is the one-line form of a reachable rule. [`why` and `reach`](query-loads.md)
