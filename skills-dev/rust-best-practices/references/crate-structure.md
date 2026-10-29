Load when you decide module boundaries or file placement inside a crate, what goes in which crate, the API between crates, or the extraction of a module into a crate. Why: boundaries decide rebuild scope, API surface, and what can be tested in isolation. Workspace layout, add-a-crate commands, and the root manifest: `references/workspace.md`. Test placement: `references/testing-and-tooling.md`.

# Crate structure: modules, layers, crate boundaries

## Files & modules
- `src/lib.rs` is the library root and public surface. `src/main.rs` is a thin binary: args → lib → exit code. Logic lives in the library so it is testable.
- A module is `foo.rs`, or `foo.rs` plus `foo/` for its children. `foo/mod.rs` still works. Pick one style per crate and do not mix them. `mod_module_files` or `self_named_module_files` can lock the choice once the crate already follows it.
- Keep items private; use `pub(crate)` when sibling modules need them. Re-export the intended public API from `lib.rs` so consumers do not depend on internal paths.
- Cargo auto-discovers `src/bin/<name>.rs`, `examples/`, `tests/`, `benches/`. Add `[[bin]]`/`[[test]]` entries only when a target needs settings (`harness = false`, `required-features`).

## Layers (dependencies point inward)
- When a crate has these layers, keep core rules independent of transport and storage, let services orchestrate them, and put DB/HTTP/filesystem adapters at the edge. A CLI or API handler converts its transport to the application API. If core policy must call an edge service, define the needed interface at the lower boundary and implement it at the edge.

## Hierarchy limits
- **God file:** more than one responsibility in one file → split by responsibility into its owning layer. Size alone is not the signal; a big pure-data file is fine. The owning rule is `octocode-clean-agentic-code/references/code-smells.md` (Structure).
- **God module/folder:** files span several domains, or flat names need prefixes (`user_api.rs`, `user_db.rs`) → one sub-module per domain.
- **Misplaced file:** business logic under `utils/`, a model under `config/`, persistence in the API layer → move it to its layer, update `use` paths, verify with build + LSP.
- Group files by the behavior their callers need; split only when the boundary improves navigation, testing, or dependency direction.

## Where to cut crates
- **By stable capability or dependency boundary.** `app-search`, `app-config`, and `app-http` can give clearer ownership than `app-models`, `app-traits`, and `app-utils`; measure a split against actual callers and build costs.
- **Along dependency weight.** Heavy deps (tokio, reqwest, oxc, napi) go in the edge crates that need them, so the core compiles fast and works without them.
- **Pure core, I/O at the edges.** Core crates take filesystem, network, env, clock, and global state as parameters or traits, so wasm, napi, and CLI reuse them.
- **Sans-IO for a protocol or parser core.** The core is a synchronous state machine: feed bytes in, poll bytes to send, and pass time in as `Instant`. The driver crate owns the socket. Quinn keeps `quinn-proto` free of sockets and drives it from Tokio. A CLI that reads files in `main` stays ordinary code.
- **Thin entry crates.** `*-cli`, `*-napi`, `*-server` parse transport → call library → map errors. Octocode separates engine algorithms, the runtime `rlib`, and the CLI host crate. The engine denies stdout/stderr printing.
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
- `crates/cli`: arguments, output, exits, and the `octocode` executable. The Node launchers run that native binary; there is no N-API host crate.
- Keep host adapters free of tool business logic and fallback execution. Check the actual manifest before assuming a dependency edge.
- Verify with `cargo metadata --format-version 1 --no-deps` (Cargo metadata, not folder names): each crate's dependencies must follow `packages/octocode-native/ARCHITECTURE.md`.

## Extract a module into a crate
1. In the current crate, make the module self-contained: only `pub(crate)` surface used by siblings, no `super::`/`crate::` reach-ins, its own error type.
2. `cargo new --lib crates/app-x`, move the files, add `app-x.workspace = true` where used.
3. Migrate callers to the intended API in the same change. Add a temporary re-export only when the requested compatibility contract requires it; Octocode prohibits unsolicited compatibility shims.
4. Verify: `cargo check --workspace --all-targets`, tests, an unused-dependency scan (`references/testing-and-tooling.md`), and `cargo build --timings` if rebuild cost motivated the split. Compare against the baseline before claiming improvement.
