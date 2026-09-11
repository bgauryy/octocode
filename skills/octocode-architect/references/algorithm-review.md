# Algorithm Review

Load when an algorithm, data structure, scheduler, parser, query, cache, retry loop, or concurrent protocol can determine correctness or cost. Why: example-based tests often miss boundary, state-space, and complexity failures.

## Contract first

Write `inputs + preconditions → postconditions + invariants → failure/termination behavior → cost model`. Identify the reference oracle when one exists. Do not optimize before the correctness contract and representative workload are explicit.

| Check | Possible flaws | Strong evidence |
|---|---|---|
| Domain and boundaries | empty/singleton input, off-by-one, invalid encoding, duplicates, ordering assumptions, aliasing, mutation of caller data | boundary partitions, exact examples, property tests |
| Invariants | state becomes unreachable or inconsistent after a transition; partial update leaks | assertions, model/state-machine tests, atomicity review |
| Termination and progress | unbounded retry/wait, recursion depth, livelock, completion that rearms itself | decreasing measure or bound, cancellation/timeout path, adversarial test |
| Correctness across paths | greedy/local choice breaks global result; stale cache/index; unstable tie-breaking | proof sketch, brute-force or trusted-oracle differential tests on small cases |
| Complexity | hidden nested scan, repeated I/O, pathological hash/tree behavior, state explosion | derive time/space in named variables, then benchmark representative and worst-shaped inputs |
| Numeric behavior | overflow/underflow, precision loss, NaN, unit or rounding error, nondeterministic floats | range analysis, typed units, tolerance/rounding contract, extreme-value tests |
| Concurrency | race, lost update, unsafe publication, deadlock, inconsistent lock order, non-idempotent replay | happens-before/ownership reasoning, atomicity contract, stress or deterministic scheduler checks |
| Randomness/distribution | modulo bias, bad seed/reproducibility, adversarial skew | statistical/property checks with fixed seeds and stated tolerances |

## Efficient proof ladder

1. Trace the smallest counterexample and each state transition in exact code.
2. State why the invariant holds or where it can break; do not call tests a proof.
3. Compare against a simple oracle, exhaustive small state space, metamorphic property, or prior implementation when practical.
4. Measure only the cost the decision names, under comparable inputs and resource limits.
5. Classify `confirmed | likely | candidate | dismissed`; unresolved input bounds, concurrency ordering, or numeric range cap the claim below confirmed.

If the flaw crosses module or ownership boundaries, continue with `references/architecture-analysis.md`. If a proven flaw needs structural change, load `references/refactoring.md`.
