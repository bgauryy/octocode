# Architecture: quality, governance and review

Load when an architecture is being chosen, reviewed or kept healthy over time: quality attributes, tenant isolation, fitness functions, architecture metrics, a review checklist or the principles behind a trade-off. Styles, patterns, roles and look-alikes are in [concepts](concepts.md); tracing and proof are in [analysis](analysis.md); refactor moves are in [change discipline](change-discipline.md).

## Choose quickly

| Problem | Start with | Watch |
|---|---|---|
| Pick an architecture | The [decision guide](concepts.md#decision-guide), checked against measured quality attributes | The simplest option that meets the requirements; distribute only for a named need |
| Requirements are vague ("fast", "scalable") | [Quality attribute scenarios](#quality-attributes) with a response measure | An attribute without a number cannot decide between options |
| Several tenants share a system | [Tenancy model](#multi-tenant-architecture): pool, bridge or silo | Physical separation does not replace per-request authorization |
| Boundaries erode between reviews | [Fitness functions](#governance-and-evolution) in the build | A rule that is not executed is a wish |
| Where to refactor first | [Hotspots](#architecture-metrics): commits × complexity from `hotspots.mjs`, change coupling | Compare scores only between runs with the same formula, window and paths |
| Reviewing an unfamiliar system or a major refactor | The [review checklist](#review-checklist) | Record what each item proved, not only that it was looked at |

## Quality attributes

Judge an architecture against measurable requirements, not pattern preference.

| Attribute | Measurement or design concern |
|---|---|
| Availability | Uptime and successful request proportion ([formula](systems-and-delivery.md#capacity-and-estimation)) |
| Reliability | Correct operation over time |
| Scalability | Throughput as demand grows |
| Elasticity | Ability to adjust resources to load |
| Latency | p50, p95 and p99 response times |
| Throughput | Requests, events or bytes per second |
| Maintainability | Cost of modifying behavior |
| Modifiability | Ease of accommodating anticipated changes |
| Deployability | Ability to release independently and safely |
| Testability | Ability to validate behavior deterministically |
| Interoperability | Compatibility across systems |
| Portability | Migration between environments |
| Recoverability | Restoration after failure (RTO, RPO) |
| Auditability | Ability to reconstruct decisions and actions |
| Evolvability | Ability to adapt the architecture over time |
| Operability | Ease of running and diagnosing production systems |
| Cost efficiency | Resource cost per business outcome |
| Sustainability | Resource and energy efficiency |

Write each requirement as a quality attribute scenario: source, stimulus, environment, artifact, response and response measure. Example: "When a zone fails (stimulus) during peak traffic (environment), checkout (artifact) keeps serving (response) with p99 under 800 ms and no lost orders (measure)." The AWS Well-Architected Framework groups the same concerns into six pillars: operational excellence, security, reliability, performance efficiency, cost optimization and sustainability.

## Multi-tenant architecture

Silo, bridge and pool are cost-versus-isolation choices; the model can differ per tier (shared compute, dedicated database).

| Model or pattern | Key consideration |
|---|---|
| Pool model | Shared resources and data structures; cheapest; isolation rests on code and policy |
| Bridge model | Partially separated tenant resources, such as a shared service with a schema per tenant |
| Silo model | Dedicated tenant resources; strongest isolation; highest cost and slowest onboarding |
| Tenant-aware routing | Route each request to the tenant's resources |
| Tenant context propagation | Carry tenant identity through every call, job and message |
| Row-level security | Restrict queries to the tenant's rows in the database itself |
| Per-tenant encryption keys | Separate cryptographic control and deletion by key destruction |
| Noisy-neighbor isolation, tenant rate limiting | Keep one tenant from consuming shared capacity; fair allocation |
| Tenant provisioning and offboarding | Automate onboarding; safe export, deletion and retention at exit |
| Tenant data migration | Move a tenant between deployments, for example from pool to silo |
| Tenant-specific configuration | Controlled customization without forking code |
| Tenant observability | Per-tenant latency, errors and cost |

Physical partitioning alone does not guarantee authorization isolation: check that every query and cache key carries the tenant and that the server derives the tenant from the authenticated identity, never from request input alone.

## Governance and evolution

| Technique | Purpose |
|---|---|
| Architecture fitness functions | Automatically verify architecture characteristics in the build |
| Dependency rules as code, module boundary enforcement, architecture tests | Reject forbidden imports and cross-module implementation access; validate layering |
| Fitness thresholds and trends | Define acceptable limits; detect deterioration over time |
| Architecture debt register | Track structural liabilities with owner and cost |
| Risk-based architecture review | Spend review effort on the most consequential decisions |
| Architecture characteristics matrix | Compare quality-attribute trade-offs across options |
| ATAM (Architecture Tradeoff Analysis Method) | Find sensitivity points, trade-off points and risks against quality attribute scenarios |
| Architecture katas | Practice trade-off-based design |
| Technical radar | Track technology adoption: adopt, trial, assess, hold |
| Architecture roadmap | Plan structural evolution in reversible steps |
| Architecture knowledge base | Preserve decision rationale ([decision record](../output.md#decision-record)) |

## Architecture metrics

Structural metrics (fan-in, fan-out, instability, abstractness) are in [dependencies and coupling](concepts.md#dependencies-and-coupling). Add runtime, delivery and change measures:

| Metric | What it reveals |
|---|---|
| Cyclic dependency count, strongly connected component size | Dependency loops and how entangled they are |
| Modularity, community detection | Natural subsystem boundaries to compare with declared ones |
| Change coupling | Files or modules that change together without a static dependency |
| Code churn, hotspot score | Edit frequency; churn combined with complexity ranks refactoring candidates |
| Propagation cost | Share of the system a change can reach through dependencies |
| API surface size | Exposed contract complexity |
| Dependency depth | Layering complexity |
| Centrality, betweenness | Modules that many dependency paths pass through |
| Deployment coupling | Components that must be released together |
| Change failure rate, lead time for changes, recovery time | Delivery health (DORA metrics) |
| Tail latency, cost per request | User-felt performance; infrastructure efficiency |
| Error-budget consumption | Reliability risk ([burn rate](systems-and-delivery.md#observability-and-sre)) |
| Architectural rule violations | Boundary compliance over time |

Rank hotspots with `node <skill-folder>/scripts/hotspots.mjs --since <YYYY-MM-DD> [pathspec ...]` ([hotspots.mjs](../scripts/hotspots.mjs); read-only git, run inside the repository). `files` are sorted by `score = commits × complexity`, where complexity is indentation depth summed over non-blank lines (`--complexity lines` counts non-blank lines instead). Each row keeps its inputs and `formula` records the calculation. Compare scores or set a threshold only between runs with the same `formula`, `--since` date and `paths`. Leave generated or vendored code out with a pathspec such as `':(exclude)dist'`. Propagation cost has several formulations: write down the exact calculation and its inputs before comparing values.

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

## Deeper topics

Areas that need their own study beyond this sheet; load primary sources for them.

| Area | Key concepts |
|---|---|
| Formal methods | TLA+, model checking, invariants, temporal logic |
| Distributed correctness | Linearizability, serializability, snapshot isolation, write skew |
| Distributed time | Clock skew, causal ordering, logical clocks, fencing |
| Formal API protocols | State machines, protocol verification, refinement |
| Real-time systems | Hard and soft deadlines, schedulability, WCET, priority inversion |
| Embedded architecture | Interrupts, DMA, RTOS, resource constraints |
| Platform engineering | Internal developer platforms, golden paths, self-service |
| Software supply chain | Signed artifacts, provenance, hermetic builds, SLSA |
| Architecture economics | Build versus buy, total cost of ownership, option value |
| Organizational architecture | Conway's law, Team Topologies, cognitive load |
| Risk modeling | FMEA, fault-tree analysis, probability × impact |
| Performance modeling | Queueing networks, load models, capacity forecasts |
| Observability design | Trace context, cardinality budgets, telemetry sampling |
| Search architecture | Index pipelines, ranking, hybrid retrieval, vector search |
| AI system architecture | RAG, inference serving, model gateways, evaluation, agent isolation |
| Data lifecycle | Archival, retention, deletion propagation, legal holds |

## Principles

1. Architecture is the management of trade-offs: every boundary, deployment model and consistency choice has a cost.
2. Dependency direction determines flexibility: protect stable business policies from volatile implementation details.
3. Logical isolation is not physical isolation: design both explicitly.
4. Distribution introduces partial failure: timeouts, retries and independent lifecycles must be designed.
5. Consistency is a business requirement: choose strong or eventual consistency from the invariants, not by default.
6. Concurrency is not parallelism: match the execution model to the I/O and CPU profile ([concurrency](concurrency.md)).
7. Scalability requires measurements: use real workload distributions, not theoretical peak throughput alone.
8. Security boundaries are architecture boundaries: authentication does not replace authorization.
9. Deployability is an architectural property: a modular codebase can still be tightly coupled operationally.
10. Architecture should be testable and observable: structural rules and quality targets become executable checks.
11. A migration is part of the design: backward compatibility and rollback matter as much as the target.
12. Prefer the simplest architecture that meets the requirements; add complexity only for a demonstrated need.
