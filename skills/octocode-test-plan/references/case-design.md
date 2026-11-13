Load after risks are ranked, when selecting techniques, levels or oracles. It turns each feature or risk into cases with an oracle; [output.md](../output.md#cases) owns the case format.

# Design test cases

## Pick techniques by risk

| Risk or behavior | Technique | Cases it yields |
|---|---|---|
| Input with ranges, sizes, formats | Equivalence partitions and boundary values | Distinct valid/invalid classes and values around boundaries that change behavior, including empty and large inputs when relevant |
| Validation, parsing, external input | Error paths and negative cases | Each rejection the code makes (read the throw, `Err` or status sites), with the exact error the user sees |
| Modes, lifecycles, retries, sessions | State transitions | Relevant allowed and forbidden transitions; repetition and interruption where state can diverge |
| Several interacting flags or settings | Decision table or pairwise | Combinations that change the outcome; use reduced combinations only with a stated coverage rationale |
| Documented behavior | Contract against docs | Trace relevant promises in docs, help text or schema to checks; report contradictions rather than testing around them |
| Changed code in a diff | Regression and impact | The changed behavior, each consumer the impact query found, and the existing tests of those consumers |
| Bug fix | Reproduction | The reported failure as a case that fails before the fix and passes after; plus its nearest boundary |
| Persistence, concurrency, resources | Invariants and stress | Restart, concurrent writers, partial failure, cleanup; give the run count or duration |
| Security-relevant input | Abuse cases | Path traversal, injection, oversize input, missing or wrong credentials, at the boundary where input enters |
| Public API or CLI surface | Compatibility | Old inputs still accepted, output shape unchanged for existing consumers |

Use the techniques and failure cases the risk needs; a priority label does not prescribe a case count.

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

Apply the [plan loop’s oracle authority rule](../SKILL.md#plan-loop): a convenient assertion does not establish expected behavior.

## Keep cases independent and stable

- Keep scenarios focused enough that a failure identifies the violated behavior.
- Pin time, randomness, locale, ordering and network in **Given**, or state the repeat count that decides a flaky case.
- Each case creates and removes its own state; shared setup lives in the Executor contract.
- Avoid duplicates across levels unless the lower level cannot observe the risk.

## Size the plan

Prioritize shipping blockers and regressions, then expand where an uncovered boundary could change acceptance. Put excluded risks in `## Not tested` with the reason. Each case must reduce a named risk; case counts do not establish coverage.
