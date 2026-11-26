Load when naming a boundary or contract term (compatibility, trust boundary, anti-corruption layer) or a dependency-injection technique and its lifetime.

# Concepts: boundaries, contracts and dependency injection

## Boundaries and contracts

| Term | Meaning | Evidence |
|---|---|---|
| C4 levels | System context (systems and people), container (deployable unit), component (module inside a container), code | One level per diagram ([presentation](../output.md#presentation-choices)) |
| Module public surface | Exports a module intends others to use | Package `exports`, barrel files, external consumers |
| Barrel | A module that re-exports others | Deep imports bypass it; check consumers before judging it |
| Port | Interface the core defines; driving (called by the outside) or driven (calls the outside) | Interface in the core, implementation outside |
| Adapter (hexagonal) | Implementation of a port for one technology; primary (inbound) or secondary (outbound); the [Gang of Four adapter](concepts-design-patterns.md#design-patterns-gang-of-four) is the class-level form | Depends inward; holds no business rule |
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
| Scoped, transient, singleton lifetime | One instance per scope, per resolution, or per application | Request-specific, stateless lightweight, stateless thread-safe shared |

Constructor injection is usually clearer than service location because required dependencies stay explicit. Construction belongs at few composition roots ([change discipline](change-discipline.md#refactor)).
