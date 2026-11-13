# Analyze a change and prove findings

Load for proof by change type, parallel review lanes or requested repairs. Pin and read the target first; the entry owns the review loop and verdict.

## Rank and order

Depth follows affected contracts: a small diff can require a full trace. State unread affected scope. Prioritize security, correctness, flow, architecture, performance, errors and maintainability by the change's risk.

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

Read the affected consumer before calling it broken; [evidence rules](evidence.md) distinguish source, semantic and runtime proof.

When an orchestrator assigns parallel lanes (Flow, Security and Errors, Architecture and Quality), each returns its findings, checked non-findings and limits. Merge by root cause and re-verify each consequential finding once. See the `octocode-workflows` composition contract.

## Repair on request

When asked to repair findings, re-read current code, apply the smallest fix per finding, rerun the checks that proved it, and report repaired, failed and outstanding items.
