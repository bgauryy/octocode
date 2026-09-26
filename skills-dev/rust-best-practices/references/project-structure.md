# Project structure — files, modules, workspace, hierarchy

Load when deciding module boundaries, file placement, crate splits, or workspace layout. Why: Rust's module/crate system rewards starting simple and splitting on real pressure; premature layering and god files both hurt.

## Single crate: files & modules
- `src/lib.rs` is the library root and public surface; `src/main.rs` is a thin binary that calls into the library. Keep logic in the library so it's testable and reusable; `main` wires args → lib → exit code.
- A module is a file (`foo.rs`) or a folder with `foo/mod.rs` **or** the modern `foo.rs` + `foo/` sibling (preferred over `mod.rs` since the 2018 style). Pick one per crate and enforce it with clippy `mod_module_files` (bans `mod.rs`) or `self_named_module_files`.
- `pub(crate)` by default; expose `pub` deliberately. Re-export the intended public API from `lib.rs` with `pub use` so consumers get a flat, stable path.
- Cargo auto-discovers targets: `src/bin/<name>.rs` (extra binaries), `examples/`, `tests/`, `benches/` — follow the convention instead of `[[bin]]`/`[[test]]` entries unless a target needs settings (`harness = false`, `required-features`).

## Tests placement
Unit tests beside the code, integration tests in one `tests/it/` binary, doc tests on public items — full layout and test kinds in `references/testing.md`.

## When to split into a workspace
When and where to split is owned by `references/crate-boundaries.md`; the npm-style mechanics (layout, npm→Cargo map, add-a-crate commands) by `references/workspace-crates.md`.

## Workspace anatomy
Virtual root `Cargo.toml` (no `[package]`) with `members = ["crates/*"]`, an explicit `resolver`, and `[workspace.package]` / `[workspace.dependencies]` / `[workspace.lints]` / `[profile.*]` — profiles only take effect at the root. Members inherit (`edition.workspace = true`, `{ workspace = true }`, `[lints] workspace = true`); one `Cargo.lock`, one `target/`. Full template and the lint-exclusivity rule: `references/workspace-manifest.md`; layout and add-a-crate commands: `references/workspace-crates.md`.

## Layered layout for larger apps
Keep dependencies pointing inward:
- **domain / core** — pure types + business rules, no I/O, no framework deps.
- **application / services** — use-cases orchestrating the domain.
- **infrastructure** — DB, HTTP, filesystem adapters (the only I/O layer).
- **presentation** — `cli/`, `api/`, or handlers; thin, converts transport ↔ application.
The domain must not depend on infrastructure; if it does, invert with a trait the infra implements.

## Hierarchy limits (borrowed from the repo's own rules)
- **God file:** > ~400 LOC *and* more than one responsibility → split by responsibility into its owning layer. Size alone isn't the signal (a big pure-data file is fine).
- **God module/folder:** many files spanning multiple domains, or names needing prefixes to disambiguate (`user_api.rs`, `user_db.rs` flat) → introduce sub-modules per domain.
- **Misplaced file:** business logic under `utils/`, a model under `config/`, persistence in the API layer → move to the correct layer, update `use` paths, verify with build + LSP.
- One file = one concern; one module = one domain.

Next: to turn a boundary into a pattern (sealed trait, typestate), load `references/design-patterns.md`; for the manifest side of a workspace, `references/workspace-manifest.md`.
