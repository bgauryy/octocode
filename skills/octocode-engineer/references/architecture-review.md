Load when reviewing an unfamiliar system, a proposed architecture or a major refactor, or when a trade-off needs the principle behind it.

# Architecture: review checklist and principles

## Review checklist

Use for an unfamiliar repository, a proposed architecture or a major refactor. For each item record the evidence and its result; [review angles](checks.md#review-angles) hold the detailed checks.

| Area | Check |
|---|---|
| Structure and boundaries | Identify modules, layers, dependencies and owners |
| | Check dependency cycles and forbidden imports |
| | Verify that contracts hide implementation details |
| | Locate god modules, hotspots and duplicated policies |
| Data and consistency | Trace reads, writes and source-of-truth ownership |
| | Inspect transaction and consistency boundaries |
| | Verify schema and API evolution compatibility |
| | Check data migrations, replay and recovery |
| Runtime and performance | Trace critical request and event paths |
| | Review concurrency, locks, cancellation and backpressure |
| | Identify bottlenecks, queues and hot partitions |
| | Validate realistic capacity and latency assumptions |
| Security and isolation | Identify trust boundaries and threat scenarios |
| | Verify authorization per resource and operation |
| | Review secrets, tenant isolation and sensitive-data handling |
| | Check supply-chain and dependency risks |
| Reliability and operations | Review timeouts, retries, idempotency and failure isolation |
| | Validate monitoring, SLIs, SLOs and alerting |
| | Verify backup restoration and disaster recovery |
| | Check deployment, rollback and graceful shutdown |
| Evolution and testing | Review tests against observable behavior |
| | Verify reversible migrations and compatibility |
| | Check architecture rules and baseline measurements |
| | Document decisions, alternatives and accepted risks |

## Principles

1. Name the cost of each boundary, deployment model and consistency choice beside its benefit; a choice with no stated cost is unreviewed.
2. Point dependencies at stable business policy; a policy module that imports a volatile implementation detail is a finding candidate.
3. Say whether each isolation claim is logical or physical, and list what the isolated units still share.
4. For each remote call, name its timeout, its retry rule and what happens while the other side is down: distribution adds partial failure and independent lifecycles.
5. Choose strong or eventual consistency per invariant, from the business rule it protects, not by default.
6. Concurrency is not parallelism: match the execution model to the I/O and CPU profile ([concurrency](concurrency.md)).
7. Size from the real workload distribution, not theoretical peak throughput alone.
8. Treat each security boundary as an architecture boundary; check authorization there even when authentication passed.
9. Check operational coupling apart from code modularity: list the components that must be released together.
10. Turn each structural rule and quality target into an executable check ([fitness functions](architecture-governance.md#governance-and-evolution)).
11. Review the migration with the target: backward compatibility and rollback for each step.
