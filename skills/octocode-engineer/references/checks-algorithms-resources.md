Load when checking an algorithm (bounds, invariants, termination, complexity, numeric or concurrency error), an efficiency claim, or memory and resource lifetime.

# Checks: algorithms, efficiency and resources

## Algorithms

- Write the contract first: `inputs + preconditions -> postconditions + invariants -> termination -> cost`; name any reference oracle and the representative workload. Example tests are evidence, not proof: they miss boundary, state-space, and complexity failures.

Flaws → strong evidence:

| Finding | Action / proof |
|---|---|
| Boundaries: empty/singleton, off-by-one, invalid encoding, duplicates, ordering assumptions, aliasing, caller-data mutation | boundary partitions, property tests. |
| Invariants: inconsistent state after a transition, leaking partial update | assertions, state-machine tests. |
| Termination: unbounded wait/poll/retry, no terminal state, recursion depth, livelock, a completion signal re-arming its own wait | decreasing measure, timeout path, adversarial test. |
| Global correctness: greedy choice, stale cache/index, unstable tie-breaking | proof sketch, oracle differential tests on small cases. |
| Complexity: hidden nested scan, repeated I/O, pathological hashing, state explosion | time/space in named variables; benchmark representative and worst-shaped inputs. |
| Numeric: overflow, precision loss, NaN, unit/rounding error, nondeterministic floats | range analysis, typed units, extreme-value tests. |
| Concurrency: race, lost update, unsafe publication, deadlock, lock-order inversion, non-idempotent replay | happens-before reasoning, stress or deterministic-scheduler checks. |
| Randomness: modulo bias, irreproducible seed, adversarial skew | fixed-seed property checks with tolerances. |

- Proof ladder: trace the smallest counterexample through exact code → state why the invariant holds or breaks → compare with an oracle, exhaustive small state space, metamorphic property, or prior implementation when practical → measure only the named cost under comparable inputs. Unresolved input bounds, ordering, or numeric range keep the claim a candidate.

## Efficiency

- Define workload size, input distribution, concurrency, cache state, and the user-visible budget. Compare equivalent conditions before and after; separate setup, cold, warm, and steady-state work.
- Attribute cost across the whole path: CPU, I/O, calls, scans, parsing, serialization, copies, allocation, synchronization, retries, and fan-out. Measure latency and throughput against the stated budget; a shorter helper or fewer lines is not an efficiency result.
- Vary input size and shape to test the claimed time and space complexity. A nested loop is a lead, not automatically quadratic in the actual input; a fast small fixture does not rule out pathological growth.
- Preserve correctness, memory bounds, cancellation, and output completeness while optimizing. Remove redundant work at its owner; do not improve a metric by dropping evidence, suppressing errors, or shifting unmeasured work to a consumer.

## Memory and resource health

- Trace allocate/acquire → owner → transfer/borrow/share → retention → release for buffers, collections, caches, tasks, subscriptions, handles, streams, and child processes. Check success, failure, timeout, cancellation, and shutdown paths; reference cycles and unsafe ownership crossings need explicit evidence.
- Measure allocated and retained memory where available, peak usage, and resource counts across repeated equivalent workloads and a quiescent period. Distinguish live retention, allocation churn, cache warm-up, allocator reservation, and operating-system RSS; one large snapshot does not prove a leak.
- Vary payload size, concurrency, and duration. Inspect unbounded queues or caches, whole-input buffering, repeated clones/copies, long-lived closures, and owners retaining larger objects than needed. Verify eviction, backpressure, streaming, and capacity limits where claimed.
- When memory safety or lifetime is in question, run the language's sanitizer or profiler ([tools](memory-safety-tools.md#debugging-and-profiling-tools)). AST and LSP can locate ownership paths; they do not establish peak usage, absence of leaks, or safe concurrent lifetime on their own.
