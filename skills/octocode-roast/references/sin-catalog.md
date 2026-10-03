# Sin catalog

Load when building or ranking the inventory.

## Tiers
| Tier | Examples |
|---|---|
| Capital offense (critical) | Confirmed credential exposure, injection/RCE, auth/access bypass; data loss/corruption; disabled security controls on a reachable production path. Requires mechanism, reachability, impact, exact evidence; redact secrets. |
| Felony (high) | N+1 or blocking work on a measured hot path; unbounded reads or memory growth; race/deadlock, swallowed failures; public-contract fragility, god units that block safe change; broad type escapes on critical boundaries. |
| Crime (medium) | Hidden state, ambiguous errors, missing tests around risky behavior; repeated duplication, boolean traps, brittle conditionals; frontend effect/dependency errors; migration, rollback, or ownership gaps with credible cost. |
| Slop (low) | Filler, comments that restate code, unclear names, blanket suppressions, dumping-ground modules, needless ceremony. |
| Misdemeanor (minor) | Stale TODOs, debug output, commented dead code, style preferences with no demonstrated impact. Mention only when signal remains. |

## Leads by ecosystem
Candidate patterns for `octocode-research`, not conclusions.

| Ecosystem | High-signal leads |
|---|---|
| TypeScript/JavaScript | repeated `any`/`@ts-ignore`, unsafe dynamic keys, `eval`, async `forEach`, unhandled promises |
| Python | bare `except` plus `pass`, mutable defaults, unsafe loaders, sync I/O in async paths |
| React | conditional hooks, missing keys, stale effect dependencies, unsafe HTML, absent error boundaries |
| SQL/data | string-built queries, unbounded reads, N+1 access, full scans on hot paths |
| Rust | unchecked `unwrap`/`panic` on user paths, blocking in async, unsafe blocks without invariants |
| Any (search families) | credential-shaped assignments, dynamic execution, disabled TLS, user input in query/path/shell; dense directories, import cycles, high fan-in/out; empty catches, ignored results; per-item network/DB calls, missing pagination; blanket disables, conflict markers |

Exclude docs, examples, fixtures, generated files, and tests unless in scope.

## Rank findings
For each candidate ask:
1. Is the mechanism proven?
2. Is the path reachable and in scope?
3. What observable consequence follows?
4. How confident is the claim?
5. What is the smallest repair?

Drop unsupported exploit, latency, or outage claims or mark them weak; never infer exploitability from syntax alone. Demote taste-only evidence to Slop or Misdemeanor.

Next: return to `references/roast-playbook.md` § 4 Autopsy.
