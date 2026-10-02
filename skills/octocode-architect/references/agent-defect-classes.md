# Agent Defect Classes

Load when reviewing agent-authored work or planning a slice an agent will implement. Why: these defects are properties of how the program runs, so a green test suite is not evidence against them.

Every class here survives functional tests by construction. Detect them structurally — dependency graph, static analysis, contract diff, clean-environment run — not by adding another test.

## Structure and boundaries

| Defect | What to inspect | Signal that it happened |
|---|---|---|
| Architectural drift | dependency edges against intended layering | edges the design forbids, added one reasonable commit at a time |
| Import cycle | graph cycles/SCCs, plus type-only and function-local imports | a cycle that did not exist at the baseline, or one routed around by a deferred import that hides it rather than removing it |
| Boundary violation | imports crossing a package or layer edge | reach into another module's internals instead of its public interface |
| Coupling growth | fan-in/fan-out on touched modules | a change in one area now propagates unpredictably |
| God module | one file or folder owning several concerns | new responsibility added to the largest existing file |
| Parallel implementation | one rule, allowlist, or schema view implemented in two places or languages | no parity fixture between the copies, or the copies already disagree on one input |
| Speculative abstraction | interfaces, factories, and config layers against their implementation count | an interface with one implementation, or a flag with one value, added for a variation point that does not exist |

Drift is invisible per commit and only measurable against intent, so state the intended boundary before the slice, then re-derive the graph after.

## Runtime correctness

| Defect | What to inspect |
|---|---|
| Thread safety | lock object identity, publication of lazily initialized state, blocking calls inside a critical section |
| Resource lifetime | every acquired handle, stream, connection, listener, subscription, timer, and child process has a close path on all branches |
| API contract | public signatures, serialized shapes, defaults, and error taxonomy against their consumers |
| Atomicity | operations that must succeed or fail together, and retry paths that must be idempotent |
| Termination | every wait, poll, and retry loop has a bound and a reachable terminal state, and no completion signal re-arms the loop that waited for it |

Three concrete thread-safety shapes worth naming, because they read as correct: lazy initialization published without a memory barrier, a lock taken on a shared or interned object rather than a private one, and a blocking sleep held inside a lock where a wait belongs.

## Deliverability

| Defect | What to inspect | Signal that it happened |
|---|---|---|
| Dependency closure | run the artifact in a clean environment using only what it declares | it runs only where undeclared packages are already installed |
| Environment coupling | values read at module scope, hardcoded infrastructure, host-specific paths | behavior changes with the working directory, host, or import order |
| Incomplete generation | reachable paths that were never implemented | a stub that returns success |
| Dependency creep | each added package against what the project already depends on | a second library for a concern an existing dependency already covers |
| Gate integrity | run the guard against a case it must reject | a check that exits zero because it parsed nothing, or a negative test that passes on an unrelated rejection, so the gate is green while inspecting nothing |

## Specification fit

Check the slice against the request, not against itself: missing corner case, wrong input type, invented object or attribute, and behavior overfitted to an example in the prompt rather than the stated rule. Also check the inverse — capability added that nobody asked for.

Next: for measured prevalence and citations load `references/agent-defect-evidence.md`; for the wiring and impact views load `references/architecture-lenses.md`.
