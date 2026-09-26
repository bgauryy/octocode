# Crate boundaries — best practices for separating packages

Load when deciding *what* goes in which crate, designing the API between crates, or extracting a module into its own crate. Why: a crate boundary is a public API, a semver unit, and a compilation unit at once — cheap to draw, expensive to move. The mechanics (layout, npm map, commands) are in `references/workspace-crates.md`.

## Where to cut
- **Cut by domain/capability, not by technical kind.** `app-search`, `app-config`, `app-http` beat `app-models`, `app-traits`, `app-utils`. A crate should answer "what does it do?", not "what kind of code is in it?".
- **Cut along dependency weight.** Put heavy deps (tokio, reqwest, oxc, napi) in the edge crates that need them so the core compiles fast and stays usable without them.
- **Pure core, I/O at the edges.** Core crates: no filesystem, network, env, clock, or global state — take them as parameters or traits. Makes the core testable and portable (wasm, napi, CLI all reuse it).
- **Thin entry crates.** `*-cli`, `*-napi`, `*-server` only parse transport → call library → map errors. Grounded: `octocode-native` keeps `crates/engine` (pure lib, denies printing) apart from `crates/runtime` (napi/CLI surface).
- **Forced splits:** a proc-macro must be its own crate (`app-macros`/`app-derive`); raw C bindings go in `foo-sys` with the safe wrapper in `foo`.
- **No `common`/`utils` grab-bag crate.** It becomes a dependency of everything, so every edit rebuilds everything. Give each helper a home domain, or name the crate after the one thing it does (`app-paths`).

## Design the boundary
- **Small public surface:** `pub(crate)` by default; `lib.rs` re-exports the intended API with `pub use`; nothing reaches in by deep path. Add `#[non_exhaustive]` to public enums/structs that will grow.
- **Don't leak dependency types.** A `reqwest::Error` or `tokio::fs::File` in a public signature makes that crate's version part of *your* API. Wrap in your own types; if re-exporting is intentional, `pub use dep;` so callers use the matching version.
- **One error enum per crate** (`thiserror`), converted with `From` at the edge; the binary collapses to `anyhow`. Never pass `anyhow::Error` across a library boundary.
- **Break cycles by moving the shared piece down**: extract a tiny `*-types` crate for shared data, or define a trait in the lower crate that the upper crate implements (dependency inversion). Never "fix" a cycle with features.
- **Generics across crates monomorphize in the caller** — great for hot paths, costly for compile time. For cold APIs take `&dyn Trait` / concrete types, or use the inner-non-generic-fn trick. Mark tiny hot cross-crate fns `#[inline]` (no LTO needed).
- **Features are additive and per-crate:** never make a feature remove API or change behavior of the default; expose them from the crate that owns the dep, and forward (`cli = ["app-core/serde"]`) rather than duplicating.

## Keep boundaries honest
- Each crate builds and tests alone: `cargo test -p <crate>` and `cargo hack check --each-feature -p <crate>` — catches hidden reliance on features another member turns on.
- Integration tests in `tests/` only see the public API — the best check that the boundary is usable.
- Enforce layering: a CI step over `cargo metadata` (or `cargo deny` `[bans]` with `wrappers`) failing if `app-core` depends on an edge crate; review `cargo tree -p app-core -e normal` when deps change.
- Internal crates: `publish = false` and version them together; published ones: `cargo semver-checks` before every release.

## Extract a module into a crate (safe order)
1. Inside the current crate, make the module self-contained: only `pub(crate)` surface used by siblings, no `super::`/`crate::` reach-ins, its own error type.
2. `cargo new --lib crates/app-x`, move the files, add `app-x.workspace = true` where used.
3. Keep old paths compiling with `pub use app_x::…;` in the old spot, migrate callers, then delete the shim.
4. Verify: `cargo check --workspace --all-targets`, tests, `cargo machete`, and `cargo build --timings` to confirm the rebuild graph improved.

Next: for layout and commands, load `references/workspace-crates.md`; for shared deps/lints, `references/workspace-manifest.md`; for traits/sealed/newtype at the boundary, `references/design-patterns.md`.
