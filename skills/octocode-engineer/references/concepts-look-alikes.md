Load before naming a role, pattern or mechanism that resembles a neighbor, to rule out the rival with the stronger claim.

# Concepts: look-alike terms

## Look-alike terms

Rule out the stronger rival on the same surface before naming ([kill the strongest alternate](analysis-topology.md#kill-the-strongest-alternate)).

| Pair | Shared surface | Distinguishing question |
|---|---|---|
| Service, facade, orchestrator | A class exposing operations | Owns the rule (service), hides a subsystem without rules (facade), or sequences several operations with state (orchestrator)? |
| Adapter, gateway, anti-corruption layer | Code at an external boundary | Converts interfaces (adapter), owns the remote call and its failures (gateway), or translates a foreign model (ACL)? |
| Repository, DAO | Persistence access | Collection of aggregates in domain language (repository) or table-shaped CRUD (DAO)? |
| DTO, value object, entity | A data-holding type | Crosses a boundary without behavior (DTO), equal by value with invariants (value object), identity over time (entity)? |
| Proxy, decorator | Wraps the same interface | Controls access or location (proxy) or adds behavior (decorator)? |
| Strategy, state, template method | Interchangeable behavior | Chosen by the caller (strategy), changes itself on transitions (state), fixed skeleton with hooks (template method)? |
| Mediator, event bus, message broker | Components talk indirectly | In-process with known participants (mediator), in-process publish (event bus), out-of-process with persistence and delivery guarantees (broker)? |
| Observer, pub/sub | Notification on change | The subject knows its observers (observer) or a channel decouples them (pub/sub)? |
| Choreography, orchestration | Multi-step distributed workflow | Each service reacts to events (choreography) or one coordinator issues commands (orchestration)? |
| DI, IoC, service locator | Dependencies supplied from outside | Passed in (DI), the framework calls you (IoC), or code asks a registry and hides the dependency (locator)? |
| Factory method, abstract factory, builder | Object creation | One product by subclass or method, families of products, or stepwise construction with options? |
| Reactor, proactor | Event-driven I/O | Dispatch on readiness (reactor) or on completion (proactor)? |
| Cache-aside, read-through, write-through | Caching | Who loads on a miss, and who writes the store? |
| Event notification, event-carried state, event sourcing | Events between components | Does the event carry an id, the state, or is it the state? |
| Command, event, query | Messages | A request to act with one handler (command), a fact that happened with many consumers (event), a read without effect (query)? |
| Layered, clean, hexagonal | Dependency rule | Toward the layer below, the use-case ring, or ports owned by the core? |
| Monolith, modular monolith, microservices | Deployment and modules | One unit without or with enforced module boundaries, or many units with their own data? |
| Middleware, interceptor, guard | Pipeline step | Transforms request flow (middleware), wraps a call (interceptor), or decides allow or deny (guard)? |
| Thread pool, worker pool, process pool | Bounded parallelism | Threads in one process, dedicated queue consumers, or separate processes? |
| Mutex, semaphore | Exclusion | Exactly one owner that releases (mutex) or a count of permits (semaphore)? |
| Idempotent consumer, inbox | Duplicate deliveries | The operation is safe to repeat (idempotent) or processed ids are stored (inbox)? |
| Unit of work, transaction script | Persistence coordination | Tracks object changes and commits once, or a procedure doing the steps? |
| Saga, distributed transaction | Cross-service consistency | Local transactions with compensation, or two-phase commit? |
| Module, package, component, container | A unit of code | Source grouping, a released unit, a C4 building block inside a deployable, or the deployable? |
