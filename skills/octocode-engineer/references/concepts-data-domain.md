Load when naming a persistence pattern (repository, unit of work, active record, data mapper) or a domain-driven design term (aggregate, bounded context, value object).

# Concepts: persistence patterns and domain-driven design

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
| Database per service | Each service owns its persistence | Independent microservices |
| Transaction script | Business operation implemented procedurally | Straightforward domain logic |
| Domain model | Rich objects encapsulate business behavior | Complex business rules |
| Soft delete | Mark records deleted instead of removing them | Audit and recovery; every query must filter |

CQRS and event sourcing are owned by [architecture styles](concepts-styles.md#architecture-styles); cache-aside, read-through, write-through and write-behind by [caching](data-systems-caching.md#read-and-write-strategies); optimistic and pessimistic locking by [concurrency](concurrency-atomics.md#synchronization-and-atomic-operations). Verify a cache by its invalidation owner and lifetime ([efficiency](checks-algorithms-resources.md#efficiency)).

## Domain-driven design

| Concept | Description | When to use |
|---|---|---|
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

Application services and repositories keep their definitions under [roles](concepts-roles.md#application-components-and-roles) and [data](#data-and-persistence-patterns); context maps and anti-corruption layers under [boundaries](concepts-boundaries.md#boundaries-and-contracts).
