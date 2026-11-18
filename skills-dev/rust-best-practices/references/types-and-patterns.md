# Types, traits and representations

Load for structs, enums, smart pointers, generics or API patterns. Start from the states and operations callers actually need; avoid translating an object model mechanically.

## Choose the representation

| Need | Candidate | Tradeoff to check |
|---|---|---|
| Mutually exclusive states with distinct data | Enum with data and exhaustive matching | Avoid invalid combinations of flags/optional fields. |
| An identifier, unit or validated value | Newtype with private fields and a validating constructor | Do not imply validation that the constructor does not perform. |
| Absence | `Option<T>` | Model distinct absence reasons explicitly when callers need them. |
| Tri-state or an ambiguous positional flag | Purpose-named enum | A clearly named boolean field/setter may already be sufficient. |
| Fixed behavior chosen at compile time | Concrete type, generic or `impl Trait` | Monomorphization can increase compile time and code size ([compile cost](code-size.md#shrink-monomorphized-code)). |
| Runtime heterogeneity or open extension | Dyn-compatible trait object | Dispatch/ownership costs versus extension needs. |
| Closed set of behaviors | Enum dispatch | Adding variants can break exhaustive downstream code. |
| Complex construction with optional inputs | Config struct or builder | Use typestate only when compile-time transitions justify complexity. |
| Public type expected to grow | Consider `#[non_exhaustive]` before publication | This changes how downstream users construct and match it. |

Keep fields private when methods enforce invariants. Plain data may have public fields. Constructors may return `Result`; do not invent a meaningless `Default` merely to derive it. Select receivers by semantics: `&self` reads, `&mut self` mutates and `self` consumes.

## Primitive choices

- Use `usize` for in-memory indexing. For persistent or wire values, follow the contract's width/range and convert with `TryFrom`; do not assume a collection fits `u32` without enforcing that invariant.
- Use `Path`/`PathBuf` and `OsStr`/`OsString` for OS paths/text. Use byte slices for bytes and convert to text only with a defined encoding policy.
- Use `Instant` for elapsed time, `Duration` for intervals and a wall-clock type for timestamps. Money needs defined units, rounding and overflow behavior; binary floats are rarely an appropriate exact representation.
- For floats, choose NaN and equality semantics explicitly. `total_cmp` provides total sorting; approximate equality requires an application-specific tolerance. `Eq`/`Hash` wrappers need a compatible documented policy.
- Derive traits only when their semantics fit. `Copy` is a public commitment and can be expensive for large values. Preserve equality/hash consistency and keep secrets out of `Debug`.

## Traits and pointers

- Keep traits cohesive. Use associated types for one natural choice per implementation and type parameters when multiple implementations are intended. Respect coherence/orphan rules; prefer extension traits for foreign types.
- Native `async fn` trait methods are not dyn-compatible on the documented stable baseline. A dyn-compatible method returning a boxed future, or an established macro, can support async dynamic dispatch. Check its allocation, `Send` and lifetime contract; do not conclude that async plugin APIs are impossible.
- Seal a public trait only when downstream implementations must be forbidden. Exposing a dependency type is a deliberate compatibility coupling, not automatically a defect.

| Ownership/access | Starting point |
|---|---|
| One owner, recursive or heap-resident data | `Box<T>` |
| Shared ownership on one thread | `Rc<T>` |
| Shared ownership across threads | `Arc<T>` when `T` satisfies the required auto traits |
| Interior mutation on one thread | `Cell` or `RefCell`; dynamic borrow conflicts can panic |
| Shared mutable state | Mutex, RwLock, channel or atomics according to the access invariant |
| Initialization once | `OnceLock`/`LazyLock` if available at the MSRV |
| Non-owning back-edge | `Weak`, or an index/handle with invalidation rules |

Prefer composition. `Deref` is for pointer-like behavior, not simulated inheritance. Avoid adding traits, generic parameters or wrappers with no demonstrated caller need. Cleanup belongs to an owned guard ([clean code](idioms.md#clean-code)).

Sources: [API Guidelines: type safety](https://rust-lang.github.io/api-guidelines/type-safety.html), [future proofing](https://rust-lang.github.io/api-guidelines/future-proofing.html), [Reference: dyn compatibility](https://doc.rust-lang.org/reference/items/traits.html#dyn-compatibility), [Arc](https://doc.rust-lang.org/std/sync/struct.Arc.html).
