# Contract and Data-Flow Checks

Load when a contract or external/internal data path crosses a trust, process, package, persistence, or ownership boundary. Why: matching types or a single happy-path test does not prove compatible semantics or safe data movement.

## Contract-level checks

Trace `declaration/schema → runtime validation → adapter/mapping → implementation → consumer → observed result`. Name the provider, consumer, owner, versioning policy, and whether the contract is public/external or internal. For each changed branch, check:

- accepted and emitted shapes, required/optional/default fields, units, ordering, encoding, nullability, and serialization;
- semantic preconditions, postconditions, invariants, authorization, side effects, and transaction boundary;
- errors, partial success, retries, timeouts, cancellation, idempotency, replay, and duplicate delivery;
- compatibility across deployed versions, persisted records, generated clients, queues/events, and consumers outside the repository;
- one valid and one invalid example through the real provider and consumer paths, plus a contract or integration test at the owning seam.

Static types prove only checked call sites. Runtime schemas prove only the boundary they execute on. Shared field names do not prove shared meaning. If the provider and consumer cannot be exercised together, report the contract as a candidate risk and name the missing evidence.

## External and internal data flows

For each representative scenario, trace `origin → ingress → validation/authentication/authorization → internal transforms → storage/cache/queue → egress/effect → observation/deletion`. At every hop record data shape, trust level, sensitivity/tenant, owner, invariant, copy, and failure behavior.

- External ingress: treat data as untrusted until the executing boundary validates and authorizes it; record normalization, provenance, rate/size limits, and rejection behavior.
- Internal movement: locate conversions, enrichment, fan-out, caches, queues, transactions, derived copies, and places where an invariant or tenant context can be lost.
- External egress: verify destination, minimization/redaction, serialization, credentials, consent/policy, retries, duplicate effects, logging, retention, and deletion.
- Feedback paths: trace callbacks, events, retries, reconciliation, and re-entry as new ingress; check ordering, replay, idempotency, and eventual-consistency windows.

Exercise normal, invalid, denied, partial-failure, retry/replay, timeout/cancellation, and stale-version paths when material. Reconcile the external trace with internal ownership: every validation, persistence, disclosure, and deletion decision needs one accountable boundary.

Return to `architecture-lenses.md` to compose this lane with static, control, ownership, and runtime views; use `output-contracts.md` when the result needs an auditable record.
