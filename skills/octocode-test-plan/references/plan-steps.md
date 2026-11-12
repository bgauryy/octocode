# Plan steps: pin, map, rank, design, write

Load to work a numbered stage of the lobby flow. The lobby's thinking modes turn each material risk into a deciding case.

## 1. Pin the target

| Input | Scope | Pin |
|---|---|---|
| Feature or module path | Its public surface and direct consumers | `git rev-parse HEAD`; on a dirty tree also `git diff HEAD \| git hash-object --stdin` |
| Local diff or branch | Changed files plus their dependents | `git diff <base>...HEAD` and base SHA |
| PR number or URL | Head revision; read it like `octocode-pr-review` reads a PR | `headCommitSha` |
| Bug report | The failing path plus a regression case | Reproduction steps as given, marked unverified |
| Spec, ticket or RFC only | Each stated requirement | Document and version; code mapping is a gap until code exists |

Record the executor: agent or person, which tools it has (shell, browser, Octocode, network), and the environment it will run in. A case the executor cannot run is a gap, not a case. Ask only when the target or executor is ambiguous and the answer changes the plan.

## 2. Understand the code

Map layout and entry points, behavior and consumers, changed scope, documented claims and existing tests. Trace `entry → validation → state → output → consumers`, exact-read deciding spans and use LSP for identity. Record unsupported promises as findings, not cases written around current behavior. Confirm commands only through read-only listing or dry-run modes.

Use the [code-map recipes](code-map.md) when a call is needed; [MCP/CLI usage](octocode-usage.md) owns invocation and recovery. [Evidence rules](evidence.md) distinguish leads, source proof and runtime observations.

## 3. Rank risks

List each feature or changed behavior with its impact (data loss, security, public contract, user-visible failure, internal only) and likelihood (new or changed code, complex branching, concurrency, external input, no current test, recent churn). Priority follows impact × likelihood: `P0` must pass to ship, `P1` should pass, `P2` adds confidence. State why each P0 is P0.

Use the [lobby's thinking modes](../SKILL.md#thinking-modes) to turn each material risk into a deciding case. Do not discard a severe reachable failure because it has not occurred.

## 4. Design cases

Derive expected results from specs or documented contracts, then types, schemas or error contracts. Current output can bless a bug; if implementation is the only authority, say so. Each oracle must let the executor compare pass/fail without guessing.

Use [case design](case-design.md) for risk-specific techniques and the cheapest level that observes the behavior. Reuse tests and fixtures; mark covered cases `existing` and give their exact commands.

Name what is out of scope and why, so the executor does not test it and a reader does not assume it was covered.

## 5. Write and check the plan

Write the plan in the [output format](../output.md). It carries the executor's Task contract, the cases and the success criteria. Before handing off, run the plan check in [output.md](../output.md#plan-check) and fix every failure. A plan with unresolved P0 gaps has status `Draft` and says what blocks it.

