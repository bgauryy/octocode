Load when picking an architecture, pattern or mechanism for a stated need and you want the candidate and the rival to rule out.

# Concepts: decision guide

## Decision guide

| Problem | Candidate | Rival to rule out |
|---|---|---|
| Single product, small or medium team | Modular monolith: clear boundaries without distributed operational overhead | Layered monolith, vertical slices |
| Independent teams and deployment needs | Microservices, when independent scaling and releases are justified | Modular monolith |
| Complex business rules | DDD with hexagonal architecture | Clean architecture |
| High-volume asynchronous workflows | Event-driven with a transactional outbox | Queue workers, workflow engine |
| Strict tenant isolation | Silo model or cells | Bridge model |
| Globally distributed traffic | Multi-region architecture | Single region with a CDN |
| Large-scale analytical workloads | Columnar warehouse or lakehouse | OLAP replicas |
| Frequent breaking API changes | Contract governance with additive evolution | Explicit versioning |
| System overloads during traffic spikes | Admission control with backpressure | Load shedding, autoscaling |
| Highly variable workload | Autoscaling with queues | Provisioned workers |
| Hide a complex subsystem behind one call | Facade | A service that also owns rules; an orchestrator with state |
| Integrate an incompatible interface | Adapter | Anti-corruption layer when a whole model differs |
| Vary an algorithm per caller | Strategy | State, when the choice changes with transitions |
| Build an object with many options | Builder | Factory, when options are fixed |
| Add behavior without subclassing | Decorator | Proxy, when the goal is access control |
| React to changes in one object | Observer | Pub/sub, when producers must not know consumers |
| Undo, queue, or log operations | Command | Memento, for state snapshots |
| Decouple the domain from persistence | Repository with a data mapper | Active record, for simple CRUD |
| Keep business rules free of frameworks | Clean or hexagonal architecture | Layered, when the domain is thin |
| Separate read and write workloads | CQRS | A read replica or materialized view |
| Audit history or rebuild state | Event sourcing | An audit log beside current state |
| Survive a failing dependency | Circuit breaker with timeout and bulkhead | Retry with backoff, for transient faults only; fallback or queue-based isolation |
| Emit events atomically with state | Transactional outbox | Dual write, which loses events |
| Handle duplicate deliveries | Idempotent consumer or inbox | Exactly-once claims by the transport |
| Multi-service workflow with rollback, long-running business transaction | Saga or workflow engine | Distributed transaction; a single database transaction when the data allows |
| Replace legacy incrementally | Strangler fig | Big-bang rewrite |
| Bound memory under load | Backpressure and bounded queues | Unbounded buffering |
| Coordinate concurrent writers | Optimistic locking | Pessimistic locking, under high contention |
| Compose dependencies | Constructor injection at a composition root | Service locator |
| Extend a product without editing the core | Plugin or microkernel with a registry | Feature flags, for temporary switches |
| Enforce a layering rule | Build-time boundary check ([model the wiring](analysis.md#model-the-wiring) lists the mechanisms) | A prose rule, which stays `inferred` |
