Load when reviewing a slice written by an agent: structure drift, runtime safety, deliverability and fit to the request.

# Checks: agent-authored work

## Agent-authored work

- A green functional suite does not rule out structural defects. For each surface the slice changed, check the dependency graph, static analysis, the contract diff and a clean-environment run. Prove a removal with the [removal proof](change-removal.md#delete-junk-and-unify).
- State the intended boundary before a slice adds or moves dependencies; re-derive the graph after.

Structure:
- Drift: forbidden edges added one reasonable commit at a time.
- Import cycle: new since baseline, or hidden by a deferred or type-only import.
- Boundary violation: reaching into internals instead of the public interface.
- Coupling growth: rising fan-in/fan-out on touched modules.
- God module: new responsibility in the largest file.
- Parallel implementation or speculative abstraction, as defined under [architecture smells](concepts-coupling.md#architecture-smells).

Runtime ([contract trace](checks.md#contract-trace) for API contracts, [Algorithms](checks-algorithms-resources.md#algorithms) for termination):
- Thread safety: lazy init without a memory barrier, lock on a shared or interned object, sleep inside a lock where a wait belongs.
- Resource lifetime: every handle, stream, connection, listener, subscription, timer, child process closes on all branches.
- Atomicity: must-succeed-together operations; idempotent retries.

Deliverability:
- Dependency closure: run in a clean environment with only declared dependencies.
- Environment coupling: behavior varies with cwd, host, or import order (module-scope reads, hardcoded infrastructure).
- Incomplete generation: a reachable stub returning success.
- Dependency creep: a second library for a covered concern.
- Gate integrity: run each guard on a case it must reject; zero exit after parsing nothing, or a negative test passing on an unrelated rejection, is a broken gate.

Spec fit: compare the slice with the request, not itself: missing corner case, wrong input type, invented object or attribute, overfitting to a prompt example, unrequested capability.
