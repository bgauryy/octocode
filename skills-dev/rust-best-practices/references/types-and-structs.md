# Types & structs — modeling data the Rust way

Load when designing a struct/enum/trait, choosing a primitive or smart pointer, or porting an OO "object" model. Why: in Rust the type *is* the design — good types delete whole classes of runtime checks; bad ones (bool flags, `Option` everywhere, `String` for everything) push bugs to runtime.

## "Objects" in Rust
- Object = `struct` (data) + `impl` (methods) + traits (shared behavior). No inheritance: compose structs, share behavior with traits and default methods.
- Constructors are plain functions: `new` (infallible), `try_new`/`parse` → `Result` (validating), `with_capacity`/`from_x`, `Default` for zero config. Big optional config → builder or `Config { a, ..Default::default() }`.
- **Private fields + validating constructor = invariant holds forever** ("parse, don't validate"). Public fields only for plain data with no invariant.
- Getters are named after the field (`fn name(&self) -> &str`, no `get_`); skip getter/setter pairs for plain data — make it a public-field struct instead.
- Methods take the weakest receiver: `&self` read, `&mut self` mutate, `self` consume/transform (builders, `into_*`).

## Pick the shape
| Modeling… | Use | Not |
|---|---|---|
| One of several states, each with its own data | `enum` with data (sum type) + `match` | struct of `Option`s + `kind: String` |
| A yes/no argument | two-variant `enum` (`Overwrite::Yes`) | `bool` param (`f(true, false)` is unreadable) |
| An id/unit/validated string | newtype `struct UserId(NonZeroU32)` / `struct Email(String)` | bare `u32`/`String` |
| "Maybe absent" | `Option<T>` | sentinel `-1`/`""`; `Option<Option<T>>` without a doc'd meaning |
| Tri-state | 3-variant enum | `Option<bool>` |
| Marker/compile-time state | unit struct, `PhantomData<State>` (typestate) | runtime flag |
| Growable API type | `#[non_exhaustive]` struct/enum | exhaustive public type you'll need to extend |

## Primitives that fit
- Index/length: `usize`; stored ids/offsets in big collections: `u32` (half the memory). Money: integer minor units or `rust_decimal`, never `f64`.
- Floats: sort with `f64::total_cmp`; don't derive `Eq`/`Hash` on float fields. Paths: `PathBuf`/`&Path` (not `String`); OS text: `OsString`; bytes: `Vec<u8>`/`&[u8]`; time: `Duration`/`Instant`, wall-clock via `jiff`/`time`.
- Conversions: `From` when lossless and infallible, `TryFrom` otherwise; never `as` for narrowing untrusted values (`u32::try_from(x)?`).

## Generics, `impl Trait`, `dyn`
- Arg position: `impl AsRef<Path>` / `impl Into<String>` for ergonomic APIs; generics `<T: Trait>` when the type is named twice or stored.
- Return: `impl Iterator<Item = T>` hides the concrete type at zero cost; `Box<dyn Trait>` only for runtime-chosen or heterogeneous values.
- Associated type (`Iterator::Item`) when there's one natural choice per impl; generic param (`From<T>`) when many impls per type are valid.
- Keep traits small and purpose-named; add an extension trait instead of growing a foreign trait; blanket impls (`impl<T: Display> MyTrait for T`) only when you own the trait.
- Derive the standard set where valid: `Debug, Clone, Copy (small, no heap), PartialEq, Eq, Hash, PartialOrd, Ord, Default`; serde derives behind a feature for libs.

## Smart pointers & interior mutability
| Need | Use |
|---|---|
| Heap/recursive/large value, single owner | `Box<T>` |
| Shared read-only, one thread / many threads | `Rc<T>` / `Arc<T>` (`Arc<str>`, `Arc<[T]>` for immutable data) |
| Mutate through `&` in one thread | `Cell<T>` (Copy) / `RefCell<T>` (runtime borrow check — panics on misuse) |
| Shared mutable across threads | `Mutex<T>` / `RwLock<T>` / atomics; or a channel instead |
| Init once, global or lazy | `OnceLock<T>` / `LazyLock<T>` (std) — not `static mut` (edition 2024 denies taking references to it) |
Lifetimes in structs (`struct View<'a> { s: &'a str }`) are fine for short-lived views/parsers; long-lived owners hold owned data or `Arc`.

Next: for named patterns built from these types, load `references/design-patterns.md`; for how types are laid out in memory, `references/memory.md`; for the JS side of a napi type, `references/napi-types.md`.
