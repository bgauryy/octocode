# Analyze a change and prove findings

Load after the target is pinned and read. It owns review depth and order, the proof each kind of change needs, clearing suspects, checks and parallel lanes.

## Rank and order

Rank by consequence: auth, persisted data, public APIs and execution configuration can need deeper review. Size or extension alone does not decide depth; tests rank below source. On a large scope, inspect high-risk paths first and record every file as read or unread.

Depth follows affected contracts and risk; a small diff can require a full trace. Work in this order: Security, Correctness, Flow, Architecture, Performance, Errors, Quality.

| Change or suspected finding | Prove with |
|---|---|
| Changed signature | `lspSearch` callers, then read the call sites |
| New function | Its callees |
| Changed type | `lspSearch` references |
| Removed export | Graph dependents when listed, LSP references, plus runtime registration, configuration, tests and external consumers |
| Module reshape | Cycles before and after |
| Security sink | Sink shape, exact read, sources and callers, guard check |
| Test gap | Changed symbol, no test references, nearby test read |
| Performance | The exact hot path; a benchmark when the claim depends on runtime |

Read a consumer exactly before calling it broken. Check APIs, schemas, dependencies, edge cases, auth, injection, data exposure, error context and hot paths. A search hit or AST match is a lead until exact source settles it; [evidence rules](evidence.md) own what each source can prove.

For each consequential finding, try to clear it: state the strongest rival and run the check that distinguishes them. Keep cleared suspects as checked non-findings, separate from findings.

Run the smallest applicable test, typecheck or lint, and report passed, failed, skipped and unavailable checks distinctly.

When an orchestrator assigns parallel lanes (Flow, Security and Errors, Architecture and Quality), each returns its findings, checked non-findings and limits. Merge by root cause and re-verify each consequential finding once. See the `octocode-workflows` composition contract.

## Repair on request

When asked to repair findings, re-read current code, apply the smallest fix per finding, rerun the checks that proved it, and report repaired, failed and outstanding items.
