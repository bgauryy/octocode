# Crate structure: modules, layers, crate boundaries

Load when you decide module boundaries or file placement inside a crate, what goes in which crate, the API between crates, or the extraction of a module into a crate. Why: boundaries decide rebuild scope, API surface, and what can be tested in isolation. Workspace layout, add-a-crate commands, and the root manifest: `references/workspace.md`. Test placement: `references/testing-and-tooling.md`.

## Files & modules
- `src/lib.rs` is the library root and public surface. `src/main.rs` is a thin binary: args → lib → exit code. Logic lives in the library so it is testable.
- A module is `foo.rs`, or `foo.rs` + `foo/` (preferred since the 2018 style), or `foo/mod.rs`. Pick one per crate; enforce it with clippy `mod_module_files` (bans `mod.rs`) or `self_named_module_files`.
- `pub(crate)` by default; `pub use` the intended API from `lib.rs` so consumers get a flat, stable path. Nothing reaches in by deep path.
- Cargo auto-discovers `src/bin/<name>.rs`, `examples/`, `tests/`, `benches/`. Add `[[bin]]`/`[[test]]` entries only when a target needs settings (`harness = false`, `required-features`).

## Layers (dependencies point inward)
- **domain / core**: pure types + business rules; no I/O, no framework deps.
- **application / services**: use-cases that orchestrate the domain.
- **infrastructure**: DB, HTTP, filesystem adapters; the only I/O layer.
- **presentation**: `cli/`, `api/`, handlers; thin, converts transport ↔ application.
- If the domain depends on infrastructure, invert it with a trait that infra implements.

## Hierarchy limits
- **God file:** > ~400 LOC *and* more than one responsibility → split by responsibility into its owning layer. Size alone is not the signal; a big pure-data file is fine.
- **God module/folder:** files span several domains, or flat names need prefixes (`user_api.rs`, `user_db.rs`) → one sub-module per domain.
- **Misplaced file:** business logic under `utils/`, a model under `config/`, persistence in the API layer → move it to its layer, update `use` paths, verify with build + LSP.
- One file = one concern; one module = one domain.

## Where to cut crates
- **By domain/capability, not technical kind.** `app-search`, `app-config`, `app-http` beat `app-models`, `app-traits`, `app-utils`. A crate answers "what does it do?".
- **Along dependency weight.** Heavy deps (tokio, reqwest, oxc, napi) go in the edge crates that need them, so the core compiles fast and works without them.
- **Pure core, I/O at the edges.** Core crates take filesystem, network, env, clock, and global state as parameters or traits, so wasm, napi, and CLI reuse them.
- **Thin entry crates.** `*-cli`, `*-napi`, `*-server` parse transport → call library → map errors. Octocode separates engine algorithms, the runtime `rlib`, and CLI/N-API host crates. The engine denies stdout/stderr printing.
- **Forced splits:** a proc-macro is its own crate (`app-macros`/`app-derive`); raw C bindings go in `foo-sys`, the safe wrapper in `foo`.
- **No `common`/`utils` grab-bag crate**: everything depends on it, so every edit rebuilds everything. Give each helper a home domain, or name the crate after its one job (`app-paths`).

## Design the boundary
- `#[non_exhaustive]` on public types that will grow.
- **Do not leak dependency types.** A `reqwest::Error` or `tokio::fs::File` in a public signature makes that crate's version part of your API. Wrap it; if re-export is intended, `pub use dep;`.
- **Break cycles by moving the shared piece down**: a tiny `*-types` crate, or a trait in the lower crate that the upper crate implements. Never fix a cycle with features.
- **Generics monomorphize in the caller**: good for hot paths, costly for compile time. Cold APIs take `&dyn Trait`/concrete types or use the inner-non-generic-fn trick.
- **Features are additive and per-crate.** A feature never removes API or changes default behavior. Expose it from the crate that owns the dep and forward it (`cli = ["app-core/serde"]`).

## Keep boundaries honest
- Each crate builds and tests alone: `cargo test -p <crate>`, `cargo hack check --each-feature -p <crate>`.
- Enforce layering in CI over `cargo metadata` (or `cargo deny` `[bans]` with `wrappers`): fail if `app-core` depends on an edge crate.
- Internal crates: `publish = false`, versioned together. Published crates: `cargo semver-checks` before every release.

## Octocode ownership example
- `crates/engine`: reusable algorithms; `crates/github`: protocol and transport. Runtime owns tool contracts, policy, configuration, credential selection, and caches.
- `crates/runtime`: request admission, cancellation, dispatch, validation, and response shaping. It has no N-API dependency or executable target.
- `crates/cli`: arguments, output, exits, and regex-worker executable. `crates/runtime-napi`: Node conversion and lifecycle over the same runtime.
- Keep host adapters free of tool business logic and fallback execution. Check the actual manifest before assuming a dependency edge.
- Verify with `yarn workspace @octocodeai/octocode-native check:crate-boundaries`; the check reads Cargo metadata rather than folder names.

## Extract a module into a crate
1. In the current crate, make the module self-contained: only `pub(crate)` surface used by siblings, no `super::`/`crate::` reach-ins, its own error type.
2. `cargo new --lib crates/app-x`, move the files, add `app-x.workspace = true` where used.
3. Keep old paths compiling with `pub use app_x::…;`, migrate callers, delete the shim.
4. Verify: `cargo check --workspace --all-targets`, tests, `cargo machete`, `cargo build --timings` (rebuild graph improved).
