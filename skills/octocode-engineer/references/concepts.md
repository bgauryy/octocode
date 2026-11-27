Load when naming what code is (a style, pattern, role, transport, or mechanism), when a term in a request, identifier, or document is ambiguous or resembles a neighbor, or when a concept claim needs its distinguishing evidence.

# Architecture concepts

A name is a hypothesis about intent. `OrderService` or a `facade/` folder is declared intent; the mechanism the code implements is observed. Use this reference to name what you see, to pick the rival explanation to rule out, and to choose the evidence that separates them.

## Contents

- [Use a concept in a review](#use-a-concept-in-a-review)
- [Architecture styles](concepts-styles.md#architecture-styles)
- [More architecture styles](concepts-styles-more.md#more-architecture-styles)
- [Dependencies and coupling](concepts-coupling.md#dependencies-and-coupling)
- [Architecture smells](concepts-coupling.md#architecture-smells)
- [Boundaries and contracts](concepts-boundaries.md#boundaries-and-contracts)
- [Object-oriented concepts and principles](concepts-oo.md#object-oriented-concepts-and-principles)
- [Design patterns (Gang of Four)](concepts-design-patterns.md#design-patterns-gang-of-four)
- [Application components and roles](concepts-roles.md#application-components-and-roles)
- [Communication](concepts-communication.md#communication)
- [Network and distributed communication](concepts-communication-network.md#network-and-distributed-communication)
- [Messaging and distributed-system patterns](concepts-messaging.md#messaging-and-distributed-system-patterns)
- [Dependency injection and lifetimes](concepts-boundaries.md#dependency-injection-and-lifetimes)
- Concurrency and parallelism: [concurrency](concurrency.md); races, termination and lock order are checked under [algorithms](checks-algorithms-resources.md#algorithms), resource lifetime under [memory and resource health](checks-algorithms-resources.md#memory-and-resource-health)
- [Data and persistence patterns](concepts-data-domain.md#data-and-persistence-patterns)
- [Domain-driven design](concepts-data-domain.md#domain-driven-design)
- [Runtime mechanics](concepts-messaging.md#runtime-mechanics)
- [Migration and refactoring vocabulary](concepts-migration.md#migration-and-refactoring-vocabulary)
- [Look-alike terms](concepts-look-alikes.md#look-alike-terms)
- [Decision guide](concepts-decision-guide.md#decision-guide)
- [Debugging and observability concepts](concepts-debugging.md#debugging-and-observability-concepts)
- [Bug categories](concepts-bugs-testing.md#bug-categories)
- [Testing concepts](concepts-bugs-testing.md#testing-concepts)

## Use a concept in a review

| Step | Do |
|---|---|
| Name | Match observed participants, edges, owned data, and lifecycle to one row; label the match `declared` (name, comment, doc) or `observed` (code). |
| Rival | Rule out the [look-alike](concepts-look-alikes.md#look-alike-terms) with a stronger claim on the same surface before naming. |
| Evidence | Styles: dependency direction and deployment units. Patterns: participants and the axis that varies. Roles: owned data, callers, and effects. Transports: framing, delivery, and failure semantics. |
| Judge | Report a misapplied or missing concept only with its consequence (the quality attribute, blast radius, or failure mode it costs), proven per the [hypothesis test](../SKILL.md#procedure). |
