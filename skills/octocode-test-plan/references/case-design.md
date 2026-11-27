Load after risks are ranked, when selecting techniques, levels or oracles. It turns each feature or risk into cases with an oracle; [levels and oracles](case-oracles.md) picks the test level and writes the oracle, and the [case format](plan-cases.md#cases) owns the fields.

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

## Keep cases independent and stable

- Give each case one behavior, so a failure names what was violated.
- Nondeterministic inputs follow the [Given field's pin rule](plan-cases.md#cases).
- Each case creates and removes its own state; shared setup lives in the Executor contract.
- Repeat one risk at a second level only when the levels observe different boundaries (the lower one cannot observe the risk).

## Size the plan

Prioritize shipping blockers and regressions, then expand where an uncovered boundary could change acceptance. Put excluded risks in `## Not tested` with the reason. Each case must reduce a named risk; neither a priority label nor a case count establishes coverage.
