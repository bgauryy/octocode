Load when reviewing the affected architecture paths for protocols, boundaries, correctness, algorithms, efficiency, resource lifetime, modularity or cleanup.

# Checks

## Review angles

Review strengths and failure hypotheses together. A healthy signal needs evidence just as a defect does; preserve the useful invariant when fixing its neighbor. For whole-system audits account for every angle, marking an angle not applicable only with a reason. For bounded work cover affected concerns without inventing unrelated work.

| Angle | Healthy signal to preserve | Failure hypothesis | Distinguishing evidence |
|---|---|---|---|
| Protocol | One contract owner, explicit state transitions, compatible producer and consumer | Shape drift, ambiguous completion, duplicate effects, lost evidence, version skew | [Contract trace](#contract-trace), [protocol transitions](#protocol-state-and-delivery), real producer/consumer replay |
| Architecture and modularity | Cohesive capabilities, explicit composition, stable dependency direction | Wrong-way dependency, leaky types, hidden effects, shared mutable state, cyclic ownership | Exact AST boundaries, LSP consumers, graph edges, declared layering rule, change impact; [look-alike terms](concepts.md#look-alike-terms) before naming a role |
| Bugs and correctness | Invariants hold on normal and exceptional paths | Off-by-one, wrong default, stale state, race, masked failure, incomplete implementation | Smallest distinguishing reproduction; assertion fails for the intended reason, then passes through the real boundary |
| Algorithms | Defined input bounds, invariants, termination, and justified data structures | Unproved greedy choice, unstable ordering, repeated scans, numeric or concurrency error | [Algorithm checks](#algorithms), oracle or property checks, adversarial inputs, complexity in named variables |
| Efficiency | Bounded work, batching or reuse with explicit lifetime and invalidation | N+1 calls, redundant parsing, serialization or copying, lock contention, retry amplification | [Efficiency checks](#efficiency), end-to-end profile, operation counts, comparable cold/warm workloads |
| Memory and resources | Clear ownership, bounded retention, cleanup on every exit | Leaks, allocation churn, excessive buffering, unbounded cache or queue, dangling ownership | [Memory checks](#memory-and-resource-health), allocation/retention evidence, repeated workload and cancellation checks |
| Junk and unification | One owner per shared policy; useful independent boundaries remain independent | Dead exports, stale wrappers, parallel rules, debug leftovers, speculative abstraction | AST candidates plus semantic and external-consumer proof; [deletion and unification](change-discipline.md#delete-junk-and-unify) |

## Contract trace

- Trace `declaration/schema -> runtime validation -> adapter/mapping -> implementation -> consumer -> observed result`; name provider, consumer, owner, versioning policy, public or internal.
- Per changed branch, check:
  - shapes, optional/default fields, units, ordering, encoding, nullability, serialization;
  - pre/postconditions, invariants, authorization, side effects, transaction boundary;
  - errors, partial success, retries, timeouts, cancellation, idempotency, replay;
  - compatibility with deployed versions, persisted records, generated clients, events, external consumers;
  - one valid and one invalid example through the real provider and consumer, plus a test at the owning seam.
- Static types prove only checked call sites, runtime schemas only their boundary, shared field names not shared meaning. If provider and consumer cannot run together, report a candidate risk naming the missing evidence.
- Tool contracts: follow input schema → adapter arguments → result shape and evidence → executable `next` pages and `hints` leads; separate measured from expected reliability. Across tools, compare shared field names and meanings; run one composition case not used in design.

## Protocol state and delivery

- Map the real states and legal transitions: initialization or negotiation, request accepted, active, partial result, completed, failed, canceled, and closed where supported. Name who owns each transition; transport acceptance is not application completion.
- Trace correlation, identity, version, ordering, timeout ownership, and error mapping across both ends. Check duplicate, delayed, out-of-order, canceled, and reconnect or replay behavior where the protocol permits them; no implicit retry of an effect without its deduplication contract.
- Check framing and serialization, backpressure, bounded queues, and partial reads or writes. For paged or streamed evidence, prove no gaps or duplicates through the final item and make terminal limits explicit.
- Verify producer and consumer together under malformed input, version skew, partial failure, and shutdown. A same-shaped payload does not prove shared meaning, authority, or lifecycle; use `octocode-agentic-prompts` when agent handoff or instruction semantics are the boundary under review.

## Data-flow trace

- Per scenario, trace `origin -> ingress -> validation/authentication/authorization -> internal transforms -> storage/cache/queue -> egress/effect -> observation/deletion`; per hop record shape, trust, sensitivity/tenant, owner, invariant, copies, failure behavior.
- Ingress: untrusted until the executing boundary validates and authorizes; normalization, provenance, size/rate limits, rejection.
- Internal movement: conversions, fan-out, caches, queues, transactions, derived copies; where invariants or tenant context get lost.
- Egress: destination, redaction, serialization, credentials, policy, retries, duplicate effects, logging, retention, deletion.
- Feedback: callbacks, events, reconciliation re-entering as ingress; ordering, replay, idempotency, consistency windows.
- Exercise the material paths: normal, invalid, denied, partial failure, retry/replay, timeout/cancellation, stale version.
- Each validation, persistence, disclosure, and deletion decision has one accountable boundary.

## Algorithms

- Write the contract first: `inputs + preconditions -> postconditions + invariants -> termination -> cost`; name any reference oracle and the representative workload. Example tests are evidence, not proof: they miss boundary, state-space, and complexity failures.

Flaws → strong evidence:
- Boundaries: empty/singleton, off-by-one, invalid encoding, duplicates, ordering assumptions, aliasing, caller-data mutation → boundary partitions, property tests.
- Invariants: inconsistent state after a transition, leaking partial update → assertions, state-machine tests.
- Termination: unbounded wait/poll/retry, no terminal state, recursion depth, livelock, a completion signal re-arming its own wait → decreasing measure, timeout path, adversarial test.
- Global correctness: greedy choice, stale cache/index, unstable tie-breaking → proof sketch, oracle differential tests on small cases.
- Complexity: hidden nested scan, repeated I/O, pathological hashing, state explosion → time/space in named variables; benchmark representative and worst-shaped inputs.
- Numeric: overflow, precision loss, NaN, unit/rounding error, nondeterministic floats → range analysis, typed units, extreme-value tests.
- Concurrency: race, lost update, unsafe publication, deadlock, lock-order inversion, non-idempotent replay → happens-before reasoning, stress or deterministic-scheduler checks.
- Randomness: modulo bias, irreproducible seed, adversarial skew → fixed-seed property checks with tolerances.

- Proof ladder: trace the smallest counterexample through exact code → state why the invariant holds or breaks → compare with an oracle, exhaustive small state space, metamorphic property, or prior implementation when practical → measure only the named cost under comparable inputs. Unresolved input bounds, ordering, or numeric range keep the claim a candidate.

## Efficiency

- Define workload size, input distribution, concurrency, cache state, and the user-visible budget. Compare equivalent conditions before and after; separate setup, cold, warm, and steady-state work.
- Attribute cost across the whole path: CPU, I/O, calls, scans, parsing, serialization, copies, allocation, synchronization, retries, and fan-out. Measure latency and throughput where relevant; a shorter helper or fewer lines is not an efficiency result.
- Vary input size and shape to test the claimed time and space complexity. A nested loop is a lead, not automatically quadratic in the actual input; a fast small fixture does not rule out pathological growth.
- Preserve correctness, memory bounds, cancellation, and output completeness while optimizing. Remove redundant work at its owner; do not improve a metric by dropping evidence, suppressing errors, or shifting unmeasured work to a consumer.

## Memory and resource health

- Trace allocate/acquire → owner → transfer/borrow/share → retention → release for buffers, collections, caches, tasks, subscriptions, handles, streams, and child processes. Check success, failure, timeout, cancellation, and shutdown paths; reference cycles and unsafe ownership crossings need explicit evidence.
- Measure allocated and retained memory where available, peak usage, and resource counts across repeated equivalent workloads and a quiescent period. Distinguish live retention, allocation churn, cache warm-up, allocator reservation, and operating-system RSS; one large snapshot does not prove a leak.
- Vary payload size, concurrency, and duration. Inspect unbounded queues or caches, whole-input buffering, repeated clones/copies, long-lived closures, and owners retaining larger objects than needed. Verify eviction, backpressure, streaming, and capacity limits where claimed.
- Use the language's appropriate diagnostics or runtime instrumentation when memory safety or lifetime is in question. AST and LSP can locate ownership paths; they do not establish peak usage, absence of leaks, or safe concurrent lifetime on their own.

## Agent-authored work

- A green functional suite does not rule out structural defects. Check dependency graphs, static analysis, contract diffs and clean-environment behavior as applicable. Removal bars: `octocode-clean-agentic-code` [code-smells](../../octocode-clean-agentic-code/references/code-smells.md).
- State the intended boundary before a slice adds or moves dependencies; re-derive the graph after.

Structure:
- Drift: forbidden edges added one reasonable commit at a time.
- Import cycle: new since baseline, or hidden by a deferred or type-only import.
- Boundary violation: reaching into internals instead of the public interface.
- Coupling growth: rising fan-in/fan-out on touched modules.
- God module: new responsibility in the largest file.
- Parallel implementation: one rule, allowlist, or schema in two places without a parity fixture.
- Speculative abstraction: one-implementation interface, one-value flag.

Runtime ([contract trace](#contract-trace) for API contracts, [Algorithms](#algorithms) for termination):
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
