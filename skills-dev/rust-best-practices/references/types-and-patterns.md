# Types and patterns

Load when you design a struct/enum/trait, choose a primitive or smart pointer, port an OO object model, or need a named pattern. Layout in memory: `references/performance-and-memory.md`. JS side of a napi type: `references/napi.md`.

## Objects
- Object = `struct` + `impl` + traits. No inheritance: compose structs; share behavior with traits and default methods.
- Constructors are functions: `new` (infallible), `try_new`/`parse` → `Result`, `with_capacity`/`from_x`, `Default`. Big optional config → builder or `Config { a, ..Default::default() }`.
- Private fields + validating constructor keep the invariant ("parse, don't validate"). Public fields only for plain data with no invariant; then skip getter/setter pairs.
- Getters take the field name (`fn name(&self) -> &str`), no `get_`.
- Weakest receiver: `&self` read, `&mut self` mutate, `self` consume (builders, `into_*`).

## Pick the shape
| Modeling | Use | Not |
|---|---|---|
| One of several states with own data | `enum` with data + `match` | struct of `Option`s + `kind: String` |
| A yes/no argument | two-variant `enum` (`Overwrite::Yes`) | `bool` param (`f(true, false)`) |
| Id, unit, validated string | newtype `struct UserId(NonZeroU32)` / `struct Email(String)` | bare `u32`/`String` |
| Maybe absent | `Option<T>` | sentinel `-1`/`""`; undocumented `Option<Option<T>>` |
| Tri-state | 3-variant enum | `Option<bool>` |
| Compile-time state | unit struct, `PhantomData<State>` (typestate) | runtime flag |
| Growable public type | `#[non_exhaustive]` | exhaustive type you must extend later |

## Primitives
- Index/length `usize`; stored ids/offsets in big collections `u32`. Money: integer minor units or `rust_decimal`, never `f64`.
- Floats: sort with `f64::total_cmp`; no `Eq`/`Hash` on float fields.
- Paths `PathBuf`/`&Path`; OS text `OsString`; bytes `Vec<u8>`/`&[u8]`; time `Duration`/`Instant`, wall clock via `jiff`/`time`.
- `From` when lossless and infallible, else `TryFrom`. Never `as` to narrow untrusted values (`u32::try_from(x)?`).

## Generics, `impl Trait`, `dyn`
- Argument: `impl AsRef<Path>` / `impl Into<String>`; generic `<T: Trait>` when the type is named twice or stored.
- Return: `impl Iterator<Item = T>`; `Box<dyn Trait>` only for runtime-chosen or heterogeneous values.
- Associated type when one natural choice per impl (`Iterator::Item`); generic param when many impls per type (`From<T>`).
- Small, purpose-named traits. Extension trait instead of growing a foreign trait. Blanket impls only on traits you own.
- Derive where valid: `Debug, Clone, Copy (small, no heap), PartialEq, Eq, Hash, PartialOrd, Ord, Default`; serde derives behind a feature in libraries.

## Smart pointers & interior mutability
| Need | Use |
|---|---|
| Heap/recursive/large, single owner | `Box<T>` |
| Shared read-only, one / many threads | `Rc<T>` / `Arc<T>` (`Arc<str>`, `Arc<[T]>`) |
| Mutate through `&`, one thread | `Cell<T>` (Copy) / `RefCell<T>` (runtime check, panics on misuse) |
| Shared mutable across threads | `Mutex<T>` / `RwLock<T>` / atomics, or a channel |
| Init once | `OnceLock<T>` / `LazyLock<T>`, not `static mut` (edition 2024 denies references to it) |

Lifetimes in structs (`struct View<'a> { s: &'a str }`) suit short-lived views and parsers; long-lived owners hold owned data or `Arc`.

## Named patterns (Rust-native, not GoF-transliterated)
| Intent | Pattern | Replaces |
|--------|---------|----------|
| Illegal states fail to compile | **Typestate** (state as a type param) | runtime `is_open` flags |
| Complex/optional-field value | **Builder** (`#[must_use]`, `build() -> Result<T>`) | telescoping constructors, many `Option` params |
| Cleanup tied to scope | **RAII guard** (work in `Drop`) | manual `close()`/defer |
| Closed set of behaviors | **Enum dispatch** + `match` | `Box<dyn Trait>` for a fixed set |
| Open, plugin-style set | **Trait objects** `Box<dyn Trait>` / generics | inheritance |
| Public trait you may extend | **Sealed trait** (private supertrait) | unversioned public trait |
| Immutable data across threads | `Arc<T>`; `Arc<Mutex<T>>` only if mutable | global singletons |
| Convert representations | `From`/`TryFrom`/`Into` | ad-hoc `to_x()` |
| Borrow-or-own return | **`Cow<'_, T>`** | always allocate |

- Typestate: `Request<Draft>` vs `Request<Sent>`; methods exist only on the allowed state. Costs type params; use it when transitions are safety-critical.
- Enum dispatch: no vtable, exhaustive `match`, inlinable. `enum_dispatch` gives trait ergonomics with enum performance.
- RAII: return the guard so the resource lives as long as the binding. `Drop` is not `async`; use explicit shutdown for critical async cleanup.
- Also: functional core, imperative shell; parse, don't validate; extension traits for foreign types.

| Classic (GoF/OO) | Rust form |
|---|---|
| Strategy | generic `<S: Strategy>` or `impl Fn(..)`; `Box<dyn Fn>` if chosen at runtime |
| State | typestate (compile time) or `enum` + `match` (runtime) |
| Command | `enum Command { … }` + one `apply` fn |
| Visitor | `match` over an enum; a `Visit` trait with default methods for big ASTs (oxc/syn) |
| Observer / event bus | channels (`mpsc`, `broadcast`), not stored callbacks with shared borrows |
| Singleton | pass a context struct; `OnceLock`/`LazyLock` only for global immutable state |
| Decorator / middleware | wrapper implementing the same trait (tower `Layer`/`Service`) |
| Dependency injection | constructor takes `impl Trait`/generic; tests pass a fake |
| Factory / template method | associated fn or `FromStr` / trait with default methods |

## Anti-patterns
- `Rc<RefCell<T>>` graphs to port an OO model → arena/indices, or `Arc` + message passing.
- Inheritance via deref chains → composition + traits.
- Deep `Box<dyn Trait>` in hot paths → generics (`references/performance-and-memory.md`).
- Stringly-typed states, `bool` params, `Option` soup → enums/newtypes (table above).
- Global mutable state (`static mut`, `lazy_static!<Mutex<…>>`) → pass context; globals defeat test isolation.
- God struct with all-`pub` fields plus accessors → split by responsibility; private fields + constructor.
- Untyped library errors → `references/idioms.md`.
- Traits with one impl, generic params nobody varies → concrete until a second use.
