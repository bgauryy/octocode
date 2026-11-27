Load when an architecture is being chosen, reviewed or kept healthy over time: quality attributes, tenant isolation, fitness functions, architecture metrics, a review checklist or the principles behind a trade-off. Styles, patterns, roles and look-alikes are in [concepts](concepts.md); tracing and proof are in [analysis](analysis.md); refactor moves are in [change discipline](change-discipline.md).

Read next: [tenancy](architecture-tenancy.md) for shared tenants; [governance and metrics](architecture-governance.md) for fitness functions and hotspots; [review checklist and principles](architecture-review.md) for a review or the principle behind a trade-off; [deeper topics](architecture-deeper-topics.md) for areas to study from primary sources.

# Architecture: quality, governance and review

## Choose quickly

| Problem | Start with | Watch |
|---|---|---|
| Pick an architecture | The [decision guide](concepts-decision-guide.md#decision-guide), checked against measured quality attributes | The simplest option that meets the requirements; distribute only for a named need |
| Requirements are vague ("fast", "scalable") | [Quality attribute scenarios](#quality-attributes) with a response measure | An attribute without a number cannot decide between options |
| Several tenants share a system | [Tenancy model](architecture-tenancy.md#multi-tenant-architecture): pool, bridge or silo | Physical separation does not replace per-request authorization |
| Boundaries erode between reviews | [Fitness functions](architecture-governance.md#governance-and-evolution) in the build | A rule that is not executed is a wish |
| Where to refactor first | [Hotspots](architecture-governance.md#architecture-metrics): commits × complexity from `hotspots.mjs`, change coupling | Compare scores only between runs with the same formula, window and paths |
| Reviewing an unfamiliar system or a major refactor | The [review checklist](architecture-review.md#review-checklist) | Record what each item proved, not only that it was looked at |

## Quality attributes

Judge an architecture against measurable requirements, not pattern preference.

| Attribute | Measurement or design concern |
|---|---|
| Availability | Uptime and successful request proportion ([formula](systems-capacity.md#capacity-and-estimation)) |
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
