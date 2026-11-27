Load when classifying a bug, or naming a test type or test double.

# Concepts: bug categories and testing

## Bug categories

| Bug | Typical symptom | Useful technique |
|---|---|---|
| Logic error | Incorrect output | Breakpoints, assertions |
| Race condition | Inconsistent results | Race detector, tracing |
| Deadlock | Execution stops progressing | Thread dumps, lock-order analysis |
| Livelock | Continuous activity without progress | Tracing, state inspection |
| Memory leak | Memory grows without bound | Heap profiler across repeated workloads |
| Null reference | Unexpected crash | Stack trace |
| Resource leak | Files or connections stay open | Resource counts on every exit path |
| Infinite loop | High CPU or nontermination | Breakpoints, CPU profiler |
| Off-by-one | Boundary failures | Boundary tests |
| Stack overflow | Excessive recursion | Call stack |
| Serialization bug | Incorrect data exchange | Payload inspection |
| Integration bug | Components disagree | Contract tests, tracing |
| Heisenbug | Behavior changes when observed | Low-overhead tracing, replay |
| Regression | Previously working behavior fails | Bisection, regression tests |

Distinguishing checks per category: [algorithms](checks-algorithms-resources.md#algorithms).

## Testing concepts

| Test type | Purpose |
|---|---|
| Unit test | One isolated unit of behavior |
| Integration test | Collaboration between components |
| Contract test | API or service agreements between provider and consumer |
| End-to-end test | Complete user workflows |
| Smoke test | Quick check of critical functionality |
| Regression test | Previously fixed bugs stay fixed |
| Characterization test | Pins current behavior before a change; passes before and after a pure refactor, fails when behavior changes |
| Snapshot test | Detects unexpected output changes |
| Property-based test | Invariants over generated inputs |
| Fuzz test | Defects under automatically generated inputs |
| Mutation test | Test effectiveness, measured by deliberate code mutations |
| Load, stress, soak test | Performance under expected load; beyond capacity; degradation over time |
| Chaos test | Behavior under infrastructure failures |
| Deterministic simulation test | Distributed and concurrency bugs, replayed from a seed that controls time, scheduling and failures |

Test behavior, not implementation details; prefer real dependencies (containers) for integration tests; run fuzzers with sanitizers; mutation testing measures test strength, not coverage; write a regression test to fail before the fix. Flaky tests usually come from time, ordering, shared state, concurrency or the network: fix the cause rather than retrying the test.

Test doubles: dummy (passed, never used), stub (canned answers), spy (records calls), mock (asserts expected calls), fake (working lightweight implementation). A double stands in for a collaborator, never for the dependency the test claims to prove ([verify the slice](change-discipline.md#verify-the-slice)).
