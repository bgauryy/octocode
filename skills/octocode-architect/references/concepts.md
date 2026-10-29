Load when naming what code is (a style, pattern, role, transport, or mechanism), when a term in a request, identifier, or document is ambiguous or resembles a neighbor, or when a concept claim needs its distinguishing evidence.

# Architecture concepts

A name is a hypothesis about intent. `OrderService` or a `facade/` folder is declared intent; the mechanism the code implements is observed. Use this reference to name what you see, to pick the rival explanation to rule out, and to choose the evidence that separates them. Every section answers three questions: which responsibility belongs where, which dependencies are permitted, and which complexity the requirements justify. A pattern present, absent, or misnamed is no finding by itself ([claims](../SKILL.md#when-making-a-claim)); a consequence is.

## Use a concept in a review

| Step | Do |
|---|---|
| Name | Match observed participants, edges, owned data, and lifecycle to one row; label the match `declared` (name, comment, doc) or `observed` (code). |
| Rival | Rule out the [look-alike](#look-alike-terms) with a stronger claim on the same surface before naming. |
| Evidence | Styles: dependency direction and deployment units. Patterns: participants and the axis that varies. Roles: owned data, callers, and effects. Transports: framing, delivery, and failure semantics. |
| Judge | Report a misapplied or missing concept only with its consequence (the quality attribute, blast radius, or failure mode it costs), proven per the [hypothesis test](../SKILL.md#steps). |

## Architecture styles

Architecture is the structure of a system: how components are organized, how responsibilities are separated, and how they communicate. Docs and folder names declare a style; dependency direction (`graph query path`, [evidence lanes](analysis.md#choose-the-narrowest-evidence-lane)) and deployment units (manifests, build and deploy config) prove it.

| Style | Description | When to use |
|---|---|---|
| Monolithic | One application containing most functionality | Small to medium applications, simple deployment |
| Modular monolith | One deployable application divided into independent modules | Most business applications requiring maintainability |
| Microservices | Independently deployed services, each owning its data | Large systems with independent scaling or teams |
| Service-oriented architecture (SOA) | Services expose reusable business capabilities | Enterprise integrations and heterogeneous systems |
| Layered (n-tier) | Separates presentation, business logic, and data access; each layer depends on the one below | Traditional enterprise applications |
| Clean architecture | Business logic independent of frameworks and infrastructure; dependencies point toward use cases and entities | Systems requiring high testability and longevity |
| Hexagonal (ports and adapters) | Domain logic communicates through ports the core owns; adapters implement them | Applications integrating multiple external systems |
| Onion architecture | Domain-centric rings with dependencies directed inward | Complex business logic |
| Event-driven architecture | Components react to published events | Asynchronous workflows and decoupled systems |
| CQRS | Separates read models from write models | Complex queries, differing read and write workloads |
| Event sourcing | Stores state changes as events; current state is derived | Auditability, event history, state reconstruction |
| Serverless | Runs functions or managed services on demand | Variable workloads, triggers, lightweight APIs |
| Client-server | Clients request functionality from servers | Web, desktop, and mobile applications |
| Peer-to-peer | Nodes communicate without one central coordinator | Distributed sharing and decentralized systems |
| Microkernel (plugin) | Core functionality extended through plugins | IDEs, extensible tools, plugin ecosystems |
| Space-based | Distributes processing and data across nodes with replicated in-memory state | Specialized high-throughput systems |
| Pipe-and-filter | Data flows through successive processing stages | Compilers, ETL, media processing |
| Blackboard | Independent components cooperate through shared state | Specialized AI and problem-solving systems |
| Microfrontend | Frontend divided into independently owned applications | Large frontend organizations with independent delivery |
| MVC | Model, view, controller; the controller updates the model, the view reads it | Web applications and UI frameworks |
| MVP | Model, view, presenter; the presenter drives a passive view | Presenter-driven and legacy UI applications |
| MVVM | Model, view, view model; the view binds to view-model state | Reactive UIs, desktop and mobile applications |
| MVI | Model, view, intent with unidirectional state flow | State-heavy reactive interfaces |

Recognize and verify:

- Layered, clean, onion, and hexagonal differ by what the dependency rule points at: the layer below, the use-case ring, the domain core, or ports the core owns. One wrong-way import breaks the claim; find it with `graph query path <outer> <inner>` and the declared rule.
- Modular monolith versus microservices is a deployment fact, not a folder fact: count deployable units and databases each unit owns.
- Event-driven names three distinct mechanisms: event notification (a thin "something happened"), event-carried state transfer (consumers keep a copy), and event sourcing (events are the store). Name which one; their consistency and replay rules differ.
- MVC, MVP, MVVM, and MVI differ by who owns UI state and the direction of updates; check who reads the model and who writes it.

Default: a modular monolith with explicit module boundaries and dependency injection. Distribute only for a concrete scaling, deployment, isolation, or organizational requirement.

## Dependencies and coupling

Vocabulary for the edges the graph shows. Metrics are component-level hypotheses; the consequence still needs a consumer or change-impact trace ([proof and ranking](analysis.md#proof-and-ranking)).

| Term | Meaning | Evidence |
|---|---|---|
| Afferent coupling (Ca) | Incoming dependents of a module | `graph query dependents <ref>`, `hubs --direction in` |
| Efferent coupling (Ce) | Outgoing dependencies of a module | `graph query deps <ref>`, `hubs --direction out` |
| Instability (I) | Ce / (Ca + Ce): 0 stable, 1 unstable | `issues --detector unstable-dependency` |
| Abstractness (A) | Share of abstract types in a component | `issues --detector main-sequence` |
| Main sequence | A + I = 1; zone of pain (stable and concrete), zone of uselessness (abstract and unused) | Same detector; confirm with real consumers |
| Cumulative component dependency (CCD, NCCD) | Sum of each file's transitive reach; normalized against a balanced tree | `issues` summary `health.ccd`, `health.nccd`, `averageReach` |
| Stable dependencies principle | Depend toward stability | A stable module importing an unstable one is a candidate |
| Stable abstractions principle | Stable modules are abstract | Interfaces live where dependents point |
| Acyclic dependencies principle | No cycles between components | `graph query cycles`, `issues --detector cycle,dir-cycle` |
| Reuse-release, common-closure, common-reuse | Package by what releases, changes, and is used together | Change history and consumer sets |
| Dependency direction versus call direction | An interface in the core is implemented and called from outside; the import points inward | Separate `imports` from `calls` edges (`--edge`) |
| Type-only edge | Import used only in type positions | `typesOnly` on `impact` rows; no runtime proof |
| Transitive dependency | Reached through another module | `walk --depth n`, `path` |
| Fan-in, fan-out | Caller and callee counts of a symbol | `callers`, `callees` |
| Re-export (barrel) edge | Dependent reaches a file through a re-exporting module | `reexportVia` on `dependents` rows |

Coupling kinds, strongest first: content (reaching into internals), common (shared global state), control (a flag that selects the callee's behavior), stamp (passing a structure for one field), data (passing only what is needed), message (communicating by messages). Cohesion is the inverse at module level: one capability per module.

Connascence names what two parts must agree on, weakest to strongest: name, type, meaning (magic values), position (argument order), algorithm (both sides implement the same rule), execution order, timing, value (invariants across copies), identity (the same instance). Strong connascence across a module boundary is a coupling candidate; inside a module it is normal.

## Architecture smells

Graph detectors emit ranked hypotheses with `verify` commands, never findings ([evidence lanes](analysis.md#choose-the-narrowest-evidence-lane)). Smells without a detector need scenarios, AST shape, and LSP references.

| Smell | What it looks like | Detector or lane |
|---|---|---|
| Cyclic dependency | Modules or directories that import each other, directly or through a chain | `cycle`, `dir-cycle`; rule out type-only edges |
| Hub-like or god module | Many incoming and outgoing edges plus mixed responsibilities | `god-file`; inspect responsibilities, not size |
| Critical file, single point of failure | Most reachable code passes through one file | `critical-file`, `single-point-of-failure`; composition roots and generated registries are expected hubs |
| Unstable dependency | A stable module depends on an unstable one | `unstable-dependency`, `main-sequence` |
| Boundary violation | An import crosses a declared layer or package boundary | `boundary-violation` with the declared rule; folder names alone are `inferred` |
| Misplaced file | A file whose dependencies belong to another module | `misplaced-file` |
| Dead or test-only code | Unreachable file, unused export, export used only locally | `unreachable-file`, `unused-export`, `export-only-local`, `test-only` with explicit `--entry`; rule out registration and external consumers |
| Undeclared or development dependency in production | Import absent from the manifest, or listed only for development | `undeclared-dependency`, `dev-dependency-in-production` |
| Unresolved import | An edge the linker could not resolve | `unresolved-import`: a coverage gap, not an absence |
| Scattered concern, shotgun surgery | One change touches many modules | `impact --since <rev>` over history; AST for the repeated shape |
| Feature envy | A module operates mostly on another module's data | `callees` and `uses` edges; LSP references |
| Leaky abstraction | Transport, storage, or framework types in a core interface | AST on signatures; [contract trace](checks.md#contract-trace) |
| Parallel implementation | One rule in two places without a parity fixture | Semantic comparison ([unify](change-discipline.md#delete-junk-and-unify)) |
| Speculative abstraction | One-implementation interface, one-value flag | `inherits` edges; consumers |

## Boundaries and contracts

| Term | Meaning | Evidence |
|---|---|---|
| C4 levels | System context (systems and people), container (deployable unit), component (module inside a container), code | One level per diagram ([presentation](../output.md#presentation-choices)) |
| Module public surface | Exports a module intends others to use | Package `exports`, barrel files, external consumers |
| Barrel | A module that re-exports others | Deep imports bypass it; check consumers before judging it |
| Port | Interface the core defines; driving (called by the outside) or driven (calls the outside) | Interface in the core, implementation outside |
| Adapter (hexagonal) | Implementation of a port for one technology; primary (inbound) or secondary (outbound); the [Gang of Four adapter](#design-patterns-gang-of-four) is the class-level form | Depends inward; holds no business rule |
| Anti-corruption layer | Translation layer protecting one model from another | Mapping code at the boundary, no foreign types inside |
| Published language | Shared schema or protocol two contexts agree on | Schema file, generated clients |
| Open host service | Protocol a context exposes for many consumers | Versioned, documented API |
| Context map relationships | Shared kernel, customer–supplier, conformist, partnership, separate ways | Who changes first and who adapts |
| Consumer-driven contract | Consumers state what they need; providers test against it | Contract tests per consumer |
| Tolerant reader | Consumer ignores unknown fields | Deserializer settings; tests with extra fields |
| Backward and forward compatibility | Old consumers read new data; new consumers read old data | Replay persisted records and deployed versions ([contract trace](checks.md#contract-trace)) |
| Semantic versioning | Major breaks, minor adds, patch fixes | Diff of the public surface against the version bump |
| Trust boundary | Where data changes trust level | Validation and authorization at the executing boundary ([data-flow trace](checks.md#data-flow-trace)) |
| Least privilege, defense in depth | Minimal rights per component; more than one control on a path | Credentials scoped per adapter; checks at more than one layer |
| Tenant boundary | Where tenant identity is enforced | Tenant in every query and cache key |
| Architecture decision record (ADR) | A settled, expensive-to-reverse choice with context and consequences | [Decision record](../output.md#decision-record) |

## Object-oriented concepts and principles

| Concept | Description | When to use |
|---|---|---|
| Class | Blueprint describing state and behavior | Defining related objects |
| Object | Runtime instance of a class | Representing entities and behavior |
| Encapsulation | Hides internal state and implementation details | Protecting invariants and controlling access |
| Abstraction | Exposes essential behavior while hiding details | Designing stable interfaces |
| Inheritance | Derives a type from another | Genuine is-a relationships |
| Polymorphism | Different implementations share an interface | Interchangeable strategies and implementations |
| Composition | Builds behavior from contained objects | The default for reusable behavior |
| Aggregation | Weak whole-part relationship | Objects with independent lifecycles |
| Association | Relationship between objects | Modeling collaborating components |
| Interface | Behavioral contract | Decoupling implementations |
| Abstract class | Shared base behavior with incomplete implementation | Related types needing shared mechanics |
| Generics | Parameterized types | Reusable, type-safe collections and algorithms |
| Delegation | Forwards responsibilities to another object | Reusing behavior without inheritance |
| Immutability | Prevents modification after construction | Predictable state, concurrency, value objects |

Entities, value objects, and aggregates are owned by [domain-driven design](#domain-driven-design).

| Principle | Meaning | Practical use |
|---|---|---|
| S: single responsibility | One primary reason for a module to change | Avoid classes with unrelated responsibilities |
| O: open/closed | Extend behavior without modifying stable code | Plugins and interchangeable policies |
| L: Liskov substitution | Subtypes honor base-type contracts | Safe inheritance and polymorphism |
| I: interface segregation | Focused interfaces | No client implements unused methods |
| D: dependency inversion | High-level policy depends on abstractions | Testable and replaceable infrastructure |

Other principles: DRY (one source per piece of knowledge, not per similar text), KISS, YAGNI, separation of concerns, information hiding, high cohesion with low coupling, law of Demeter (talk only to immediate collaborators), composition over inheritance, tell don't ask, least astonishment, fail fast.

Recognize and verify: an inheritance claim holds only if every subtype honors the base contract; check overrides that weaken postconditions or throw. Composition shows as owned fields with delegated calls. A DRY violation is two copies of one rule with shared change pressure, not two similar texts.

## Design patterns (Gang of Four)

| Creational | Purpose | When to use |
|---|---|---|
| Factory method | Delegates object creation to subclasses or specialized methods | Creation varies by concrete type |
| Abstract factory | Creates families of related objects | Multiple compatible implementations |
| Builder | Constructs complex objects step by step | Many optional construction parameters |
| Prototype | Creates objects by copying existing ones | Cloning configured objects |
| Singleton | Restricts a type to one instance | A genuinely unique application-wide resource |

| Structural | Purpose | When to use |
|---|---|---|
| Facade | Simplified interface to a complex subsystem | Hide complicated library or subsystem operations |
| Adapter | Converts one interface into another | Integrating incompatible APIs |
| Bridge | Separates an abstraction from its implementation | Two independently varying dimensions |
| Composite | Treats individual objects and groups uniformly | Trees, UI components, file systems |
| Decorator | Adds behavior by wrapping an object | Logging, compression, instrumentation |
| Proxy | Controls access to another object | Remote access, caching, authorization, lazy loading |
| Flyweight | Shares reusable intrinsic state | Very large numbers of similar objects |

| Behavioral | Purpose | When to use |
|---|---|---|
| Strategy | Interchangeable algorithms | Different pricing, sorting, or validation policies |
| Observer | Notifies subscribers of changes | Events, UI updates, state observation |
| Command | Encapsulates an operation as an object | Undo and redo, job execution, operation queues |
| State | Changes behavior based on current state | Workflows, protocol state machines |
| Template method | Algorithm skeleton with customizable steps | Shared algorithms with controlled variation |
| Chain of responsibility | Passes requests through handlers | Middleware, validation, request pipelines |
| Mediator | Coordinates object interactions | Complex communication among components |
| Memento | Captures and restores previous state | Undo and checkpoints |
| Iterator | Traverses collections without exposing representation | Custom data structures |
| Visitor | Adds operations to a stable object hierarchy | ASTs, compilers, document structures |
| Interpreter | Represents and evaluates grammar rules | Small DSLs and expression languages |

Recognize and verify:

- A pattern is its participants plus the axis that varies: strategy varies the algorithm, state varies with transitions, template method fixes the skeleton. Same class shapes, different reasons to change.
- A class named for a pattern that lacks its participants (a `Factory` that constructs one fixed type, a `Singleton` that tests reset) is declared, not observed.
- Patterns are tools, not findings. Report one only when its absence or misuse costs a named quality attribute.

## Application components and roles

Roles are responsibilities, not class names. Verify a role by what it owns (data, effects, invariants) and who calls it.

| Component | Responsibility | When to use |
|---|---|---|
| Controller | Receives requests and coordinates application logic | HTTP endpoints, MVC |
| Service | Performs application or business operations | Reusable use cases and business workflows |
| DAO | Direct data access operations | SQL-heavy or persistence-focused applications |
| DTO | Transfers data between boundaries | APIs, IPC, remote calls |
| Mapper | Converts between models | Domain, database, and transport conversion |
| Application service | Coordinates one use case | Workflows across domain and infrastructure |
| Infrastructure service | Interfaces with technical capabilities | File system, email, storage, external API |
| Use case (interactor) | Encapsulates one application operation | Clean architecture |
| Handler | Processes one command, event, or request | CQRS, messaging, HTTP |
| Middleware | Processes requests in a pipeline | Authentication, logging, rate limits |
| Interceptor | Wraps method calls or operations | Tracing, authorization, instrumentation |
| Guard | Decides whether an operation is permitted | Authorization, routes, access control |
| Validator | Enforces constraints | Input validation and business rules |
| Policy | Encapsulates an operational or business decision | Permissions, pricing rules |
| Gateway | Owns the call to an external service and its failures | Third-party services and remote systems |
| Provider | Supplies dependencies or platform-specific capabilities | DI frameworks and platform abstraction |
| Coordinator | Coordinates navigation or UI workflows | Complex UI flows |
| Orchestrator | Controls a multistep process | Business workflows spanning components |
| Scheduler | Triggers work according to timing rules | Periodic or delayed jobs |
| Worker | Executes asynchronous or background tasks | Queues and CPU-intensive jobs |
| Dispatcher | Routes work or events to handlers | Event loops and message systems |
| Serializer | Converts objects into transferable data | JSON, Protocol Buffers, binary formats |
| Event bus | Delivers events between in-process components | Loosely coupled notifications |
| API gateway | Central entry point for multiple backend services | Distributed API infrastructure |
| BFF (backend for frontend) | Backend tailored to one frontend | Web and mobile clients with differing API requirements |

Facades are defined under [design patterns](#design-patterns-gang-of-four); domain services under [domain-driven design](#domain-driven-design); repositories, units of work, specifications, and caches under [data and persistence](#data-and-persistence-patterns); adapters and anti-corruption layers under [boundaries and contracts](#boundaries-and-contracts); message brokers under [messaging](#messaging-and-distributed-system-patterns).

## Communication

Choose by requirement: direct calls inside one process, IPC between local processes, HTTP or gRPC across services, messaging for asynchronous work or temporal decoupling. Verify a transport by its framing, delivery guarantee, ordering, timeout owner, and failure mapping ([protocol state and delivery](checks.md#protocol-state-and-delivery)).

Delivery guarantees: at-most-once (fire and forget), at-least-once (retry until acknowledged; duplicates possible), effectively exactly-once (at-least-once plus an idempotent or deduplicating consumer; no transport provides it alone). Ordering holds per partition, key, or connection, not globally, unless the protocol states otherwise.

### Inter-process communication (IPC)

| Mechanism | Description | When to use |
|---|---|---|
| Pipes | Unidirectional byte stream | Parent-child processes, command pipelines |
| Named pipes (FIFO) | Named channel between processes | Local process communication |
| Unix domain sockets | Local sockets through the socket interface | Local services on Unix-like systems |
| TCP sockets | Network-capable bidirectional connections | Local or remote communication |
| UDP sockets | Connectionless datagrams | Low-latency communication, telemetry |
| Shared memory | Several processes map the same memory | High-throughput data exchange |
| Memory-mapped files | File-backed or anonymous mappings | Large datasets, shared buffers |
| Message queues | Discrete messages between processes | Asynchronous process communication |
| Signals | Lightweight operating-system notifications | Process lifecycle and control |
| RPC | Invokes functionality in another process | Service-like process boundaries |
| D-Bus | Desktop and system message bus | Linux desktop and system services |
| COM | Component interaction technology | Windows application integration |
| XPC | Structured process communication | macOS and Apple platform services |
| Binder | Android's primary IPC mechanism | Android system services |
| Electron IPC | Messages between main and renderer processes | Electron desktop applications |
| stdio (JSON-RPC, MCP) | Framed messages over a child process's standard streams | Tool servers, language servers, agent protocols |

Semaphores, mutexes, and condition variables coordinate access; they transport nothing and are listed under [concurrency](#concurrency-and-parallelism).

### Network and distributed communication

| Technology or pattern | Description | When to use |
|---|---|---|
| REST | Resource-oriented HTTP interface style | Public APIs, CRUD operations |
| GraphQL | Clients query the data they need | Data-rich frontends |
| gRPC | Contract-based remote procedure calls | Internal, strongly typed service communication |
| JSON-RPC | Lightweight RPC protocol | Tools, internal APIs, local IPC |
| XML-RPC | RPC encoded as XML | Legacy integrations |
| SOAP | XML-based messaging protocol | Enterprise and legacy systems |
| WebSocket | Persistent bidirectional connection | Real-time messaging |
| Server-sent events (SSE) | Server-to-client streaming over HTTP | Live dashboards and progress updates |
| WebRTC data channels | Peer-to-peer data transport | Interactive peer communication |
| HTTP polling | Periodic requests for updated state | Simple or low-frequency updates |
| Long polling | Server holds a request until data is available | Notifications without persistent sockets |
| Pub/sub | Publishers send events to subscribers through a channel | Event-driven decoupling |
| Request/response | Sender requests an operation and receives a result | Synchronous queries |
| Fire-and-forget | Sender does not wait for completion | Noncritical asynchronous notifications |
| Request/reply messaging | Correlated request and response messages | Asynchronous RPC-like workflows |
| Message streaming | Continuous ordered record processing | Analytics, pipelines, event processing |
| Webhooks | HTTP callbacks on events | Integrations with external platforms |
| Brokered messaging | Messages pass through a broker | Reliable asynchronous service integration |

## Messaging and distributed-system patterns

| Pattern | Description | When to use |
|---|---|---|
| Publish/subscribe | One event may reach multiple consumers | Event notifications |
| Message router, content-based router | Routes messages by rules or by content | Integration pipelines, conditional routing |
| Message translator | Changes message formats | System integration |
| Message filter | Discards irrelevant messages | Event pipelines |
| Aggregator, splitter | Combines messages into one result; breaks one into units | Parallel processing, batch processing |
| Competing consumers | Several consumers read one queue | Horizontal scaling of work |
| Claim check | Store a large payload and pass a reference | Large messages |
| Dead letter queue (DLQ) | Stores messages that cannot be processed | Recovering failed message operations |
| Transactional outbox | Persists business data and outgoing events in one transaction | Avoiding lost events (versus dual write) |
| Inbox | Tracks processed incoming message ids | Consumer deduplication |
| Idempotent consumer | Safely handles duplicate deliveries | At-least-once messaging |
| Idempotency key | Caller-supplied id that makes a retried request a no-op | Payments, retried commands |
| Saga | Distributed workflow as local transactions with compensations | Multi-service business workflows |
| Choreography | Services react to each other's events | Decentralized workflows |
| Orchestration | A coordinator directs workflow steps | Explicit workflow management |
| Message broker | Intermediary that stores and delivers messages | Distributed messaging |
| Circuit breaker | Stops calls to a failing dependency for a period | Prevent cascading failures |
| Retry with backoff | Retries transient failures with increasing delays and jitter | Network instability |
| Bulkhead | Isolates resource pools or workloads | Prevent failure propagation |
| Timeout | Bounds how long an operation may take | External calls and remote operations |
| Rate limiting | Restricts request frequency | API protection and quotas |
| Load balancing | Distributes traffic across instances | Scaling and availability |
| Service discovery | Resolves available service instances | Dynamic distributed environments |
| Sidecar | Runs supporting functionality beside an application | Proxies, telemetry, service mesh |
| Leader election | Selects a coordinating node | Distributed coordination |
| Distributed lock | Exclusive ownership across nodes | Rare shared-resource coordination |
| Sharding | Partitions data between nodes | Large datasets and high scale |
| Strong, eventual, causal consistency | Readers see the latest write; converge later; see writes in causal order | Pick per data item; name the window a consumer can observe |

## Dependency injection and lifetimes

| Pattern or technique | Description | When to use |
|---|---|---|
| Dependency injection (DI) | Supplies dependencies from outside an object | Testability and decoupling |
| Constructor injection | Passes dependencies through constructors | Default for required dependencies |
| Setter injection | Supplies dependencies after construction | Optional or reconfigurable dependencies |
| Method injection | Supplies dependencies for one operation | Operation-specific requirements |
| Inversion of control (IoC) | An external mechanism controls creation or execution | Frameworks and application bootstrapping |
| DI container | Constructs and wires dependencies | Large dependency graphs |
| Service locator | Finds dependencies through a central registry; hides them from signatures | Legacy systems or limited infrastructure |
| Composition root | The place where dependencies are assembled | Application initialization; an expected hub |
| Factory | Encapsulates creation logic | Runtime-dependent construction |
| Lazy initialization | Defers creation until first use | Expensive, rarely needed resources |
| Object pool | Reuses expensive instances | Connections and heavyweight resources |
| Scoped, transient, singleton lifetime | One instance per scope, per resolution, or per application | Request-specific, stateless lightweight, stateless thread-safe shared |

Constructor injection is usually clearer than service location because required dependencies stay explicit. Construction belongs at few composition roots ([change discipline](change-discipline.md#refactor)).

## Concurrency and parallelism

| Concept or pattern | Description | When to use |
|---|---|---|
| Thread | Execution sequence within a process | Concurrent or parallel execution |
| Thread pool | Reuses worker threads | Many short tasks |
| Process pool | Distributes work across processes | CPU-intensive workloads and isolation |
| Async/await | Expresses asynchronous operations sequentially | I/O-bound operations |
| Future, promise | An eventual result | Async APIs |
| Reactive streams | Asynchronous data streams with backpressure | Continuous data |
| Actor model | Isolated state with message-based interaction | Concurrent stateful systems |
| Producer–consumer | Producers create work for consumers | Job pipelines |
| Work queue | Buffers tasks for later execution | Asynchronous work distribution |
| Worker pool | Fixed workers process queued work | Bounded parallelism |
| Fork–join | Splits tasks and combines results | Parallel recursive computations |
| MapReduce | Parallel transformation and aggregation | Large-scale batch processing |
| Mutex (lock) | Serializes access to shared state; one owner releases | Protecting critical sections |
| Read–write lock | Concurrent reads, exclusive writes | Read-heavy shared structures |
| Semaphore | Limits concurrent access to a count of permits | Resource pools, concurrency limits |
| Condition variable | Waits for a state change under a lock | Coordinating shared-memory consumers |
| Atomic operations, compare-and-swap | Indivisible updates; conditional atomic update | Counters, lock-free structures, optimistic algorithms |
| Memory barrier, happens-before | Ordering guarantees between threads | Lazy initialization, publication of shared state |
| Thread-local storage | Per-thread state | Thread-specific contexts |
| Immutable state | No mutation after construction | Reducing synchronization |
| Cancellation token | Cooperative cancellation | Long-running tasks |
| Backpressure | Slows production when consumers are overloaded | Streams and high-throughput systems |
| Structured concurrency | Child-task lifetimes tied to a parent scope | Reliable async task management |
| Event loop | Dispatches events and asynchronous callbacks | Node.js, GUI frameworks |
| Reactor | Dispatches readiness events to handlers | Network servers |
| Proactor | Dispatches completed asynchronous operations | Asynchronous I/O frameworks |

Termination, races, and lock-order inversion are checked under [algorithms](checks.md#algorithms); resource lifetime under [memory and resource health](checks.md#memory-and-resource-health).

## Data and persistence patterns

| Pattern | Description | When to use |
|---|---|---|
| Active record | Objects combine data with persistence operations | Simple database-backed applications |
| Data mapper | Separates objects from persistence mechanics | Complex domain models |
| Repository | Collection-like persistence abstraction over aggregates | Domain-centered applications |
| Unit of work | Tracks changes and commits them once | Transaction boundaries |
| Identity map | One object per identity within a scope | ORM sessions |
| Lazy loading, eager loading | Load related data on demand, or in advance | Expensive relationships; avoiding repeated queries |
| Query object | Encapsulates query logic | Complex reusable queries |
| Specification | Combinable business criteria | Composable filtering and domain rules |
| Materialized view | Stores precomputed query results | Read optimization |
| Cache-aside | Application checks the cache, then the source, and fills the cache | Read-heavy workloads |
| Read-through cache | The cache loads missing data | Centralized cache loading |
| Write-through cache | Updates cache and backing store together | Simplified read consistency |
| Write-behind cache | Defers backing-store writes | High write throughput with controlled risk |
| Optimistic locking | Detects conflicting concurrent changes by version | Low-contention updates |
| Pessimistic locking | Locks data before changes | Operations requiring exclusive access |
| Database per service | Each service owns its persistence | Independent microservices |
| Transaction script | Business operation implemented procedurally | Straightforward domain logic |
| Domain model | Rich objects encapsulate business behavior | Complex business rules |
| Schema migration (expand, migrate, contract) | Add the new shape, move data and readers, remove the old | Zero-downtime schema change |
| Soft delete | Mark records deleted instead of removing them | Audit and recovery; every query must filter |

CQRS and event sourcing are owned by [architecture styles](#architecture-styles). Verify a cache by its invalidation owner and lifetime ([efficiency](checks.md#efficiency)).

## Domain-driven design

| Concept | Description | When to use |
|---|---|---|
| Domain | The business problem space | Understanding application purpose |
| Subdomain: core, supporting, generic | Distinct business capability by strategic value | Deciding where to invest design effort |
| Bounded context | Boundary within which a model has one consistent meaning | Separating business models |
| Ubiquitous language | Shared terminology between developers and domain experts | Reducing ambiguity |
| Entity | Domain object with identity over time | Order, customer, subscription |
| Value object | Domain concept defined by its value, immutable | Money, address |
| Aggregate | Transactional consistency boundary around related objects | Related entities with shared invariants |
| Aggregate root | Entry point controlling an aggregate | Enforcing consistency |
| Domain event | Records something meaningful that happened | Decoupling business reactions |
| Domain service | Domain operation that belongs to no one entity | Complex business calculations |
| Domain policy | Encapsulates a domain decision | Pricing and eligibility rules |
| Domain factory | Constructs valid domain objects | Complex domain creation |

Application services and repositories keep their definitions under [roles](#application-components-and-roles) and [data](#data-and-persistence-patterns); context maps and anti-corruption layers under [boundaries](#boundaries-and-contracts).

## Runtime mechanics and resilience

| Pattern or concept | Description | When to use |
|---|---|---|
| Feature flags | Enable functionality conditionally | Gradual rollouts; remove after rollout |
| Plugin system, plugin loader | Discovers and loads optional functionality | Extensible products |
| Middleware pipeline | Ordered request processing stages | Servers, CLI tools |
| State machine, finite state machine | Valid states and transitions | Protocols, workflows, UI flows |
| Hierarchical state machine | Nested states | Complex workflows |
| Rule engine | Evaluates configurable rules | Dynamic business policies |
| Workflow engine | Executes defined business processes | Long-running workflows |
| Dependency graph | Dependencies between components | Startup, build, and task ordering |
| Registry | Maps identifiers to implementations | Handler discovery; an expected hub |
| Context object | Carries request-scoped information | Request ids, deadlines, cancellation |
| Null object | Safe default behavior | Removing some null checks |
| Result (either), option (maybe) | Explicit success or failure; explicit absence | Recoverable errors; null-safe APIs |
| RAII, resource scope | Cleanup tied to object lifetime or a scope | Files, connections, locks |
| Health check | Reports whether a component can operate | Monitoring and orchestration |
| Graceful shutdown | Stop intake, drain in-flight work, release resources | Services and workers |

## Migration and refactoring vocabulary

Terms used by [change discipline](change-discipline.md#refactor), which owns the moves and guardrails.

| Term | Meaning | Evidence it worked |
|---|---|---|
| Seam | A place where behavior can be changed without editing the code in place | A [characterization test](#testing-concepts) can substitute one side |
| Strangler fig | Route traffic to new code path by path, then retire the old | Both paths measured; old path removed after consumer checks |
| Branch by abstraction | Introduce an abstraction, move implementations behind it, swap | No duplicated callers at the end |
| Parallel change (expand, migrate, contract) | Add the new shape, move consumers, remove the old | Persisted data and deployed versions read both shapes during migration |
| Feature toggle | Runtime switch selecting a path | Both paths tested; toggle removed after rollout |
| Sprout, wrap | Add new behavior in a new unit, or around the old one | Old unit unchanged; new behavior tested at the seam |
| Shim, compatibility layer | Adapter that keeps an old contract alive | Consumer list and retirement check; a candidate for removal once consumers moved |
| Vertical slice | One scenario moved end to end | Scenario passes through the new path with the old removed for it |

## Look-alike terms

Rule out the stronger rival on the same surface before naming ([kill the strongest alternate](analysis.md#kill-the-strongest-alternate)).

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

## Decision guide

| Problem | Candidate | Rival to rule out |
|---|---|---|
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
| Survive a failing dependency | Circuit breaker with timeout and bulkhead | Retry with backoff, for transient faults only |
| Emit events atomically with state | Transactional outbox | Dual write, which loses events |
| Handle duplicate deliveries | Idempotent consumer or inbox | Exactly-once claims by the transport |
| Multi-service workflow with rollback | Saga | Distributed transaction |
| Replace legacy incrementally | Strangler fig | Big-bang rewrite |
| Bound memory under load | Backpressure and bounded queues | Unbounded buffering |
| Coordinate concurrent writers | Optimistic locking | Pessimistic locking, under high contention |
| Compose dependencies | Constructor injection at a composition root | Service locator |
| Extend a product without editing the core | Plugin or microkernel with a registry | Feature flags, for temporary switches |
| Enforce a layering rule | Build-time boundary check (a boundary test over the module tree, `cargo metadata` or `cargo deny` bans for crate edges) | A prose rule, which stays `inferred` |

## Debugging and observability concepts

Debugging evidence is runtime evidence; it establishes the loaded artifact and scenario, not every configuration ([evidence](evidence.md#match-proof-to-the-claim)).

| Concept | Purpose | When to use |
|---|---|---|
| Breakpoint, conditional breakpoint | Pause execution at a line, or only when a condition holds | Inspect specific behavior; bugs under specific conditions |
| Watchpoint (data breakpoint) | Pause when a variable or memory changes | Unexpected state mutations |
| Logpoint | Log without stopping execution | Live execution |
| Step into, step over, step out | Enter a call, run it whole, or finish the current function | Follow control flow at the needed depth |
| Call stack, stack trace | Active calls; recorded call sequence | Find where an error originated; analyze crashes |
| Variable inspection, expression evaluation | Inspect runtime values; run expressions while paused | Find incorrect state; test assumptions |
| Core dump | Snapshot of a crashed process's memory | Native application crashes |
| CPU profiling (sampling, instrumentation) | Identify expensive execution paths; sampling is cheap, instrumentation is exact | Performance bottlenecks |
| Memory profiling, heap snapshot | Analyze allocations and retained objects | Memory leaks and retention |
| Flame graph | Visualize execution cost by stack | CPU and latency analysis |
| Logs, metrics, traces | The three observability signals: events, aggregates, request paths | Production debugging |
| Structured logging | Machine-readable events with fields | Searchable production logs |
| Distributed tracing | Follow one request across services | Microservices debugging |
| Correlation id | Connect logs belonging to one operation | Multi-component requests |
| Assertions | Validate expected invariants | Detect invalid states early |
| Sanitizers, race detector | Detect memory errors; detect unsafe concurrent access | Native code; multithreaded programs |
| Static analysis | Detect issues without executing code | CI and development |
| Symbolic debugging | Map machine addresses to source | Native binaries and crash dumps |
| Remote debugging | Debug another process or machine | Containers, servers, devices |
| Time-travel debugging | Replay execution history | Intermittent bugs |
| Bisection (`git bisect`) | Find the commit that introduced a bug | Regressions |
| Delta debugging | Minimize a failing input or configuration | Complex reproductions |
| Fault injection | Deliberately introduce failures | Resilience validation |
| Root cause analysis | Determine the underlying cause | Recurring or significant incidents |

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
| Timeout | Operations exceed a deadline | Latency profiling |
| Heisenbug | Behavior changes when observed | Low-overhead tracing, replay |
| Regression | Previously working behavior fails | Bisection, regression tests |

Distinguishing checks per category: [algorithms](checks.md#algorithms).

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

Test doubles: dummy (passed, never used), stub (canned answers), spy (records calls), mock (asserts expected calls), fake (working lightweight implementation). A double stands in for a collaborator, never for the dependency the test claims to prove ([verify the slice](change-discipline.md#verify-the-slice)). Example tests are evidence, not proof ([algorithms](checks.md#algorithms)).

## Repository layouts

A folder layout is a layering candidate, `inferred` until a build rule, lint, or package boundary enforces it ([model the wiring](analysis.md#model-the-wiring)). Verify a layout's claim with `graph query path` across the folders it names, and `issues --detector boundary-violation,misplaced-file`.

Layered, for straightforward APIs and CRUD applications:

```text
my-app/
├── src/
│   ├── controllers/user.controller.ts
│   ├── services/user.service.ts
│   ├── repositories/user.repository.ts
│   ├── models/user.model.ts
│   ├── dtos/
│   ├── middleware/
│   ├── config/
│   └── main.ts
├── tests/
└── package.json
```

Feature-based modular monolith, the default for most medium-sized projects:

```text
my-app/
├── src/
│   ├── modules/
│   │   ├── users/
│   │   │   ├── user.controller.ts
│   │   │   ├── user.service.ts
│   │   │   ├── user.repository.ts
│   │   │   ├── user.model.ts
│   │   │   └── user.test.ts
│   │   ├── orders/
│   │   └── payments/
│   ├── shared/
│   │   ├── errors/
│   │   ├── logging/
│   │   └── config/
│   └── main.ts
├── tests/
└── package.json
```

Clean or hexagonal, for complex business logic with several infrastructure integrations. Infrastructure and presentation depend on application and domain abstractions, never the reverse:

```text
my-app/
├── src/
│   ├── domain/
│   │   ├── entities/
│   │   ├── value-objects/
│   │   ├── services/
│   │   └── events/
│   ├── application/
│   │   ├── use-cases/
│   │   ├── ports/
│   │   └── dtos/
│   ├── infrastructure/
│   │   ├── persistence/
│   │   ├── messaging/
│   │   └── external-apis/
│   ├── presentation/
│   │   ├── controllers/
│   │   └── middleware/
│   └── main.ts
├── tests/
└── package.json
```

DDD with feature modules, for large business domains:

```text
my-app/
├── src/
│   ├── modules/
│   │   ├── orders/
│   │   │   ├── domain/        # entities, value-objects, events, repositories (interfaces)
│   │   │   ├── application/   # commands, queries, handlers
│   │   │   ├── infrastructure/ # persistence, messaging
│   │   │   └── presentation/  # controllers
│   │   ├── payments/
│   │   └── inventory/
│   ├── shared/
│   └── main.ts
└── tests/
```

Microservices in a monorepo, when services genuinely need independent deployment. Share stable contracts, never internal domain models:

```text
platform/
├── services/
│   ├── user-service/
│   │   ├── src/
│   │   ├── tests/
│   │   └── Dockerfile
│   ├── order-service/
│   └── payment-service/
├── packages/
│   ├── contracts/
│   ├── observability/
│   └── tooling/
├── infrastructure/
│   ├── docker/
│   ├── kubernetes/
│   └── terraform/
├── scripts/
└── README.md
```

Desktop application with IPC (Electron, Tauri, multi-process). The flow is renderer, preload API, IPC handler, service; expose a narrow, validated IPC API instead of privileged access:

```text
desktop-app/
├── src/
│   ├── main/
│   │   ├── ipc/file.handler.ts
│   │   ├── services/file.service.ts
│   │   └── main.ts
│   ├── preload/index.ts
│   ├── renderer/
│   │   ├── components/
│   │   ├── features/
│   │   ├── hooks/
│   │   └── app.tsx
│   └── shared/
│       ├── ipc-contracts.ts
│       └── types.ts
├── tests/
└── package.json
```

Frontend with several features (React, Vue, Angular):

```text
frontend/
├── src/
│   ├── app/
│   │   ├── routes/
│   │   └── providers/
│   ├── features/
│   │   ├── auth/
│   │   │   ├── components/
│   │   │   ├── hooks/
│   │   │   ├── api/
│   │   │   └── types.ts
│   │   └── dashboard/
│   ├── components/ui/
│   ├── lib/
│   ├── assets/
│   └── main.tsx
├── public/
└── package.json
```

Engineering support, whatever the architecture: explicit homes for tests, configuration, documentation, diagnostics, and automation:

```text
project/
├── src/
├── tests/
│   ├── unit/
│   ├── integration/
│   ├── e2e/
│   └── fixtures/
├── scripts/
│   ├── dev/
│   └── diagnostics/
├── config/
├── docs/
│   ├── architecture/
│   ├── adr/
│   └── runbooks/
├── .github/workflows/
├── .vscode/launch.json
├── .env.example
├── Dockerfile
├── README.md
└── CONTRIBUTING.md
```

A practical default combines feature-based modules, enforced boundaries, colocated unit tests, centralized integration and end-to-end tests, structured logging, correlation ids, and reproducible debugging configurations.
