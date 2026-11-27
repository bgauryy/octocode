Load when naming or verifying an architecture style: monolith, microservices, layered, clean, hexagonal, event-driven, CQRS, event sourcing or a UI pattern (MVC, MVP, MVVM, MVI).

# Concepts: architecture styles

## Architecture styles

Docs and folder names declare a style; dependency direction (graphQuery `path`, [evidence lanes](analysis-topology.md#choose-the-narrowest-evidence-lane)) and deployment units (manifests, build and deploy config) prove it.

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

- Layered, clean, onion, and hexagonal differ by what the dependency rule points at: the layer below, the use-case ring, the domain core, or ports the core owns. One wrong-way import breaks the claim; find it with graphQuery `path` from the outer to the inner module and the declared rule.
- Modular monolith versus microservices is a deployment fact, not a folder fact: count deployable units and databases each unit owns.
- Event-driven names three distinct mechanisms: event notification (a thin "something happened"), event-carried state transfer (consumers keep a copy), and event sourcing (events are the store). Name which one; their consistency and replay rules differ.
- MVC, MVP, MVVM, and MVI differ by who owns UI state and the direction of updates; check who reads the model and who writes it.

Default: a modular monolith with explicit module boundaries and dependency injection. Distribute only for a concrete scaling, deployment, isolation, or organizational requirement.
