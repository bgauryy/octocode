Load when reviewing the affected architecture paths for protocols, boundaries, correctness, algorithms, efficiency, resource lifetime, modularity or cleanup.

Read next: [algorithms, efficiency and resources](checks-algorithms-resources.md) for an algorithm, cost or resource-lifetime check; [agent-authored work](checks-agent-work.md) when reviewing a slice an agent wrote.

# Checks

## Review angles

Review strengths and failure hypotheses together; preserve useful invariants while fixing their neighbors. Cover the requested audit scope and affected concerns. The angles below guide investigation, not a required report checklist.

| Angle | Healthy signal to preserve | Failure hypothesis | Distinguishing evidence |
|---|---|---|---|
| Protocol | One contract owner, explicit state transitions, compatible producer and consumer | Shape drift, ambiguous completion, duplicate effects, lost evidence, version skew | [Contract trace](#contract-trace), [protocol transitions](#protocol-state-and-delivery), real producer/consumer replay |
| Architecture and modularity | Cohesive capabilities, explicit composition, stable dependency direction | Wrong-way dependency, leaky types, hidden effects, shared mutable state, cyclic ownership | Exact AST boundaries, LSP consumers, graph edges, declared layering rule, change impact; [look-alike terms](concepts-look-alikes.md#look-alike-terms) before naming a role |
| Bugs and correctness | Invariants hold on normal and exceptional paths | Off-by-one, wrong default, stale state, race, masked failure, incomplete implementation | Smallest distinguishing reproduction; assertion fails for the intended reason, then passes through the real boundary |
| Algorithms | Defined input bounds, invariants, termination, and justified data structures | Unproved greedy choice, unstable ordering, repeated scans, numeric or concurrency error | [Algorithm checks](checks-algorithms-resources.md#algorithms), oracle or property checks, adversarial inputs, complexity in named variables |
| Efficiency | Bounded work, batching or reuse with explicit lifetime and invalidation | N+1 calls, redundant parsing, serialization or copying, lock contention, retry amplification | [Efficiency checks](checks-algorithms-resources.md#efficiency), end-to-end profile, operation counts, comparable cold/warm workloads |
| Memory and resources | Clear ownership, bounded retention, cleanup on every exit | Leaks, allocation churn, excessive buffering, unbounded cache or queue, dangling ownership | [Memory checks](checks-algorithms-resources.md#memory-and-resource-health), allocation/retention evidence, repeated workload and cancellation checks |
| Junk and unification | One owner per shared policy; useful independent boundaries remain independent | Dead exports, stale wrappers, parallel rules, debug leftovers, speculative abstraction | AST candidates plus semantic and external-consumer proof; [deletion and unification](change-removal.md#delete-junk-and-unify) |

## Contract trace

- Trace `declaration/schema -> runtime validation -> adapter/mapping -> implementation -> consumer -> observed result`; name provider, consumer, owner, versioning policy, public or internal.
- Per changed branch, check:
  - shapes, optional/default fields, units, ordering, encoding, nullability, serialization;
  - pre/postconditions, invariants, authorization, side effects, transaction boundary;
  - errors, partial success, retries, timeouts, cancellation, idempotency, replay;
  - compatibility with deployed versions, persisted records, generated clients, events, external consumers;
  - representative accepted and rejected inputs through the real provider and consumer, with regression coverage at the owning seam.
- Static types prove only checked call sites, runtime schemas only their boundary, shared field names not shared meaning. If provider and consumer cannot run together, report a candidate risk naming the missing evidence.
- Tool contracts: follow input schema → adapter arguments → result shape and evidence → executable `next` pages and `hints` leads. Compare shared field meanings and exercise downstream composition when interoperability is claimed.

## Protocol state and delivery

- Map the real states and legal transitions: initialization or negotiation, request accepted, active, partial result, completed, failed, canceled, and closed where supported. Name who owns each transition; transport acceptance is not application completion.
- Trace correlation, identity, version, ordering, timeout ownership, and error mapping across both ends. Check duplicate, delayed, out-of-order, canceled, and reconnect or replay behavior where the protocol permits them; no implicit retry of an effect without its deduplication contract.
- Check framing and serialization, backpressure, bounded queues, and partial reads or writes. For paged or streamed evidence, prove no gaps or duplicates through the final item and make terminal limits explicit.
- Verify producer and consumer together under malformed input, version skew, partial failure, and shutdown. A same-shaped payload does not prove shared meaning, authority, or lifecycle; use `octocode-agentic` when agent handoff or instruction semantics are the boundary under review.

## Data-flow trace

- Per scenario, trace `origin -> ingress -> validation/authentication/authorization -> internal transforms -> storage/cache/queue -> egress/effect -> observation/deletion`; per hop record shape, trust, sensitivity/tenant, owner, invariant, copies, failure behavior.
- Ingress: untrusted until the executing boundary validates and authorizes; normalization, provenance, size/rate limits, rejection.
- Internal movement: conversions, fan-out, caches, queues, transactions, derived copies; where invariants or tenant context get lost.
- Egress: destination, redaction, serialization, credentials, policy, retries, duplicate effects, logging, retention, deletion.
- Feedback: callbacks, events, reconciliation re-entering as ingress; ordering, replay, idempotency, consistency windows.
- Exercise the material paths: normal, invalid, denied, partial failure, retry/replay, timeout/cancellation, stale version.
- Each validation, persistence, disclosure, and deletion decision has one accountable boundary.
