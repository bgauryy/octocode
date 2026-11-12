Load at step 4 of the skill, after risks are ranked. It turns each feature or risk into cases with an oracle; [output.md](../output.md#cases) owns the case format.

# Design test cases

## Pick techniques by risk

| Risk or behavior | Technique | Cases it yields |
|---|---|---|
| Input with ranges, sizes, formats | Equivalence partitions and boundary values | One case per valid and invalid partition; values at, just below and just above each boundary; empty, one, many, maximum |
| Validation, parsing, external input | Error paths and negative cases | Each rejection the code makes (read the throw, `Err` or status sites), with the exact error the user sees |
| Modes, lifecycles, retries, sessions | State transitions | Each allowed transition and at least one forbidden one; repeat and interrupt mid-transition |
| Several interacting flags or settings | Decision table or pairwise | One case per rule that changes the outcome; pairwise when combinations explode |
| Documented behavior | Contract against docs | One case per promise in docs, help text or schema; a contradiction is reported, not tested around |
| Changed code in a diff | Regression and impact | The changed behavior, each consumer the impact query found, and the existing tests of those consumers |
| Bug fix | Reproduction | The reported failure as a case that fails before the fix and passes after; plus its nearest boundary |
| Persistence, concurrency, resources | Invariants and stress | Restart, concurrent writers, partial failure, cleanup; give the run count or duration |
| Security-relevant input | Abuse cases | Path traversal, injection, oversize input, missing or wrong credentials, at the boundary where input enters |
| Public API or CLI surface | Compatibility | Old inputs still accepted, output shape unchanged for existing consumers |

Use the techniques the risk needs, not all of them. Each P0 gets at least one negative or error-path case.

## Pick the level

Choose the cheapest level that observes the behavior:

| Level | Observes | Use when |
|---|---|---|
| Unit | One function or module with its inputs | Logic, boundaries and error mapping inside one unit |
| Integration | Real collaborators: file system, parser, database, process | The risk is in the seam between units or in configuration |
| End-to-end | The built artifact through its public entry (CLI, HTTP, UI) | The claim is user-visible behavior, packaging or wiring; a case that runs the real program on fixture files is end-to-end |
| Manual or exploratory | A person or agent judging an open question | No automatic oracle exists; give a time box and what to look for |

Prefer existing harnesses and fixtures. A new fixture is part of the case's **Given**: say where it goes and what it contains.

## Write the oracle

The oracle is how the executor decides pass or fail. Prefer, in order:

1. Exact value: exit code, status code, an exact string or JSON field value.
2. Pattern or property: output matches a regex, a list is sorted, a count equals the input count, a round trip returns the input.
3. Differential: the new path gives the same result as the old version, a reference implementation or a second tool.
4. Bound: a timing or size bound with unit, baseline and number of runs.
5. Human judgment: only for manual cases, with the question to answer.

When no expected value can be computed (the oracle problem: ranking, numeric models, generated output), use a property or metamorphic relation: a change to the input with a known effect on the output, such as reordering input leaves a sorted result unchanged.

Apply the [entry’s oracle authority rule](plan-steps.md#4-design-cases) before selecting an assertion below.

## Keep cases independent and stable

- One behavior per case. A case that checks three things fails without saying which.
- Pin time, randomness, locale, ordering and network in **Given**, or state the repeat count that decides a flaky case.
- Each case creates and removes its own state; shared setup lives in the Executor contract.
- Avoid duplicates across levels unless the lower level cannot observe the risk.

## Size the plan

Cover every P0 risk fully, P1 risks with their main case and one negative case, and P2 risks only when cheap. Put the rest in `## Not tested` with the reason. A larger plan is not a better plan: each case must name the risk it reduces.
