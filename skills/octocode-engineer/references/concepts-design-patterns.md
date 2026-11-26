Load when naming or verifying a Gang of Four design pattern (creational, structural or behavioral).

# Concepts: design patterns

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

- A pattern is its participants plus the axis that varies: the same class shapes can have different reasons to change. Separate strategy, state and template method with [look-alike terms](concepts-look-alikes.md#look-alike-terms).
- A class named for a pattern that lacks its participants (a `Factory` that constructs one fixed type, a `Singleton` that tests reset) is declared, not observed.
