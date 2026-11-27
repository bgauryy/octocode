Load when naming what a component is: controller, service, DAO, mapper, handler, middleware, gateway, orchestrator, worker or a similar role.

# Concepts: application components and roles

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

Facades are defined under [design patterns](concepts-design-patterns.md#design-patterns-gang-of-four); domain services under [domain-driven design](concepts-data-domain.md#domain-driven-design); repositories, units of work, specifications, and caches under [data and persistence](concepts-data-domain.md#data-and-persistence-patterns); adapters and anti-corruption layers under [boundaries and contracts](concepts-boundaries.md#boundaries-and-contracts); message brokers under [messaging](concepts-messaging.md#messaging-and-distributed-system-patterns).
