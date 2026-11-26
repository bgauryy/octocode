Load when an object-oriented concept or principle (the SOLID letters, composition, inheritance, encapsulation, DRY) is claimed or disputed.

# Concepts: object-oriented principles

## Object-oriented concepts and principles

| Concept | Description | When to use |
|---|---|---|
| Encapsulation | Hides internal state and implementation details | Protecting invariants and controlling access |
| Abstraction | Exposes essential behavior while hiding details | Designing stable interfaces |
| Inheritance | Derives a type from another | Genuine is-a relationships |
| Polymorphism | Different implementations share an interface | Interchangeable strategies and implementations |
| Composition | Builds behavior from contained objects | The default for reusable behavior |
| Aggregation | Weak whole-part relationship | Objects with independent lifecycles |
| Interface | Behavioral contract | Decoupling implementations |
| Abstract class | Shared base behavior with incomplete implementation | Related types needing shared mechanics |
| Generics | Parameterized types | Reusable, type-safe collections and algorithms |
| Delegation | Forwards responsibilities to another object | Reusing behavior without inheritance |
| Immutability | Prevents modification after construction | Predictable state, concurrency, value objects |

Entities, value objects, and aggregates are owned by [domain-driven design](concepts-data-domain.md#domain-driven-design).

| Principle | Meaning | Practical use |
|---|---|---|
| S: single responsibility | One primary reason for a module to change | Avoid classes with unrelated responsibilities |
| O: open/closed | Extend behavior without modifying stable code | Plugins and interchangeable policies |
| L: Liskov substitution | Subtypes honor base-type contracts | Safe inheritance and polymorphism |
| I: interface segregation | Focused interfaces | No client implements unused methods |
| D: dependency inversion | High-level policy depends on abstractions | Testable and replaceable infrastructure |

Other principles: DRY, KISS, YAGNI, separation of concerns, information hiding, high cohesion with low coupling, law of Demeter (talk only to immediate collaborators), composition over inheritance, tell don't ask, least astonishment, fail fast.

Recognize and verify: an inheritance claim holds only if every subtype honors the base contract; check overrides that weaken postconditions or throw. Composition shows as owned fields with delegated calls. A DRY violation is two copies of one rule with shared change pressure, not two similar texts ([duplication proof](analysis-proof.md#proof-and-ranking)).
