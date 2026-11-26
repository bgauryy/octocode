Load when checking code quality, change risk, retest scope or migration order from a saved graph.

# Code-quality checks and multi-query flows

## Code-quality checks

Batch the first queries of a check in one load (`--stdin`), then read only the cited lines.

| Check | Ask | Signal | Next step |
|---|---|---|---|
| Critical functions to test first | `critical functions 30` | High PageRank or many transitive callers | `callers <fn> --tests`: `stats.testCalls` counts its callers in test files; 0 means no test calls it directly |
| Choke points | `top betweenness 10`, `critical points` | One file carries many routes or holds a part together | `about <file>`; give it an owner and tests, or split the interface it exposes |
| God files | `top inweight 20`, `about <file>` | Heavy use plus many declarations whose callers sit in different communities | Split along the communities its callers come from |
| Cycles | `cycle <file>`, `order <file> --imports` | A set that must move together; ring rows carry each hop's weight | Cut the lightest crossing edge; confirm at its cited line |
| Stability violations | `metrics.packages`, `diff` (`againstStability`) | A stable package (low I) depends on a volatile one | Invert the dependency or move the shared code down |
| Change risk | `impact <file> depth 3`, `critical path <file> --up` | Many dependents or a long chain above | Run the tests on that chain first |
| Layering | `model.json` layers, then `octocode-graph render` | `upward_imports` in the brief | Read each upward import at its line |
| Dead code candidates | `about <file>` (0 dependents), native `issues` | No dependents and not an entry point | Confirm with `lspSearch` references; dynamic loading is invisible |
| Module boundaries | `community <file>` | Low purity: the community spans chunks | Compare with the packages a change touches |
| Architecture rule ("the CLI reaches storage only through the service layer") | `path <cli-file> <store-file> --avoid <service-dir>` | A route found: the rule is broken at each cited hop | Fix the crossing import, or record an allowed exception in `model.json` |
| A critical flow | `region <entry> <outcome>` (files) or `region <a#fn> <b#fn>` (functions) | `mustPass` steps: checkpoints a change cannot route around | Test the checkpoints; `path --avoid` one to prove an alternative exists |

## Flows that need more than one query

| Question | Flow |
|---|---|
| What should we retest? | `impact <file...> depth 2` lists dependents; `impact <file> --tests` adds the tests that reach it. Native `impact` with `changed` or `since: "main"` lists `testsToRun` and `affectedEntrypoints`. Record actual coverage or a test run separately. |
| Which seed reaches a consumer? | One `impact a b depth 2` gives each file its minimum distance from any seed; ask one query per seed over `--stdin` to attribute it. |
| Fewest boundaries or strongest coupling? | `path a b --hops` against `path a b` ([path](query-walks.md)): a strongly coupled route can be longer. Keep the same filters for both. |
| What should move together? | `cycle <file> --imports`, then `order <file> --imports`: condensation groups each strongly connected set; read the crossing witnesses before choosing the first group to move. |
| Did two modules evolve together? | Native `cochange` on a file gives commit coupling; compare it with direct imports, and with `diff` across builds for structure. |
| Is a hotspot slow, untested or on a mandatory path? | Attach a [provider](providers.md) (coverage, control flow, traces) and `trace test:<id> COVERS` or `trace workload:<name> OBSERVED EXECUTES_FILE`. Mandatory-path claims need control-flow evidence. |

Native detectors run on one snapshot scope with `graphQuery` `issues`: [operations](operations.md) lists them, and [read the rows](build-and-read.md#read-the-rows) says how to check a finding.

Rows are evidence candidates: before a claim, confirm each as [evidence](evidence.md) says.
