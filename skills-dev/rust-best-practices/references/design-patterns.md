# Design patterns — Rust-native, not GoF-transliterated

Load when a design question calls for a named pattern. Why: half of classic OO patterns dissolve into Rust's type system; using the Rust-native form prevents fighting the language.

## Pick by intent

| Intent | Pattern | Replaces |
|--------|---------|----------|
| Prevent mixing up like-typed values | **Newtype** `struct Meters(f64)` | bare primitives, runtime asserts |
| Make illegal states unrepresentable at compile time | **Typestate** (state as a type param) | runtime `is_open` flags |
| Construct a complex/optional-field value | **Builder** (`#[must_use]`, `build() -> Result<T>`) | telescoping constructors |
| Deterministic cleanup tied to scope | **RAII guard** (work in `Drop`) | manual close()/defer |
| Closed set of behaviors, no dynamic dispatch | **Enum dispatch** + `match` | `Box<dyn Trait>` when the set is fixed |
| Open set of behaviors, plugin-style | **Trait objects** `Box<dyn Trait>` / generics | inheritance |
| A public trait you may extend later without breaking callers | **Sealed trait** | unversioned public trait |
| Share immutable data across threads | `Arc<T>`; `Arc<Mutex<T>>` only if mutable | global singletons |
| Convert between representations ergonomically | `From`/`TryFrom`/`Into` | ad-hoc `to_x()` methods |
| Zero-cost "borrow or own" return | **`Cow<'_, T>`** | always-allocate |

## Notes that matter
- **Typestate** encodes a state machine in types: `Request<Draft>` vs `Request<Sent>`; methods exist only on the state that allows them, so a misuse won't compile. Cost: more type params — use when the transitions are safety-critical.
- **Enum dispatch beats trait objects** when the variant set is closed and known: no vtable, exhaustive `match`, inlinable. Switch to `dyn Trait`/generics only for genuinely open extension. `enum_dispatch` crate bridges the two when you want trait ergonomics with enum performance.
- **Builder**: mark it `#[must_use]`, make `build()` return `Result` when validation can fail, and prefer it over 5-argument constructors or many `Option` params.
- **RAII guards** (like `MutexGuard`): put teardown in `Drop`; return the guard so the resource lives exactly as long as the binding. Don't rely on `Drop` for critical async cleanup — it isn't `async` (use explicit shutdown).
- **Sealed trait**: add a private supertrait so outside crates can't implement your public trait, leaving you free to add methods later without a breaking change.

## Anti-patterns to steer away from
- Reaching for `Rc<RefCell<T>>` graphs to port an OO object model — usually a sign the ownership design needs rethinking (arena/index, or `Arc` + message passing).
- Inheritance emulation via deref chains — prefer composition + traits.
- Deep `Box<dyn Trait>` in hot paths — monomorphize with generics instead (`references/performance.md`).
- Stringly-typed states/config — newtype or enum them.

Next: for how these types shape signatures and conversions, load `references/idioms.md`; for the ownership-graph escape hatches, `references/gotchas.md`.
