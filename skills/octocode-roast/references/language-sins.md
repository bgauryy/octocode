# Language-Specific Sins and Search Patterns

Load when generic categories need language-specific leads. Why: syntax varies, but every cited finding still needs exact impact and confidence.
Pair with `sin-catalog.md`; use these as candidate patterns for `octocode-research`, not conclusions.

## Examples by ecosystem
| Ecosystem | High-signal leads |
|---|---|
| TypeScript/JavaScript | repeated `any`/`@ts-ignore`, unsafe dynamic keys, `eval`, async `forEach`, unhandled promises |
| Python | bare `except` plus `pass`, mutable defaults, unsafe loaders, sync I/O in async paths |
| React | conditional hooks, missing keys, stale effect dependencies, unsafe HTML, absent error boundaries |
| SQL/data | string-built queries, unbounded reads, N+1 access, full scans on hot paths |
| Rust | unchecked `unwrap`/`panic` on user paths, blocking in async, unsafe blocks without invariants |

## Search families
| Category | Candidate patterns |
|---|---|
| Security | credential-shaped assignments, dynamic execution, unsafe HTML, disabled TLS, user input in query/path/shell |
| Architecture | large mixed-responsibility units, dense directories, import climbs, cycles, high fan-in/fan-out |
| Types/errors | broad escapes, suppressed diagnostics, empty catches, panic/unwrap, ignored results |
| Performance/data | sync hot-path I/O, unbounded loops/reads, per-item network/DB calls, missing pagination |
| Quality/residue | stale TODO/FIXME, blanket disables, debug output, conflict markers, generated-looking filler |

Exclude docs, examples, fixtures, generated files, and tests before ranking unless they are in scope.
Ask `octocode-research` to upgrade each lead to exact evidence, mechanism, impact, and confidence.
Demote taste-only evidence to Slop or Misdemeanor; never infer exploitability or production impact from syntax alone.
