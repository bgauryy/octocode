Load when writing or reading one rule: its fields, load modes, capture groups, `allowed` and `required`.

# Rule fields

## Rule fields

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
