# Project structure — files, modules, workspace, hierarchy

Load when deciding module boundaries, file placement, crate splits, or workspace layout. Why: Rust's module/crate system rewards starting simple and splitting on real pressure; premature layering and god files both hurt.

## Single crate: files & modules
- `src/lib.rs` is the library root and public surface; `src/main.rs` is a thin binary that calls into the library. Keep logic in the library so it's testable and reusable; `main` wires args → lib → exit code.
- A module is a file (`foo.rs`) or a folder with `foo/mod.rs` **or** the modern `foo.rs` + `foo/` sibling (preferred over `mod.rs` since the 2018 style). Pick one convention per crate and hold it.
- `pub(crate)` by default; expose `pub` deliberately. Re-export the intended public API from `lib.rs` with `pub use` so consumers get a flat, stable path.
- Binaries beyond one: `src/bin/<name>.rs` or `[[bin]]` entries; examples in `examples/`, benches in `benches/`.

## Tests placement
- Unit tests live next to the code in `#[cfg(test)] mod tests { ... }` — they can reach private items.
- Integration tests live in `tests/` — each file is its own crate that sees only the public API, doubling as a usability check on that API.
- Doc tests in `///` examples run under `cargo test` — keep them compiling.

## When to split into a workspace
Split a crate out when: compile times bite and a stable lower layer rarely changes; a piece is independently publishable/reusable; or two areas have genuinely separate dependency sets. Don't split on taste alone.

## Workspace anatomy (grounded template)
```toml
# root Cargo.toml
[workspace]
members = ["crates/engine", "crates/runtime"]
default-members = ["crates/runtime"]
resolver = "2"

[workspace.package]             # shared edition/MSRV/license — ONE edition for the whole workspace
edition = "2024"
rust-version = "1.89"

[workspace.dependencies]        # unify versions once, inherit in members
serde = { version = "1", features = ["derive"] }

[workspace.lints.clippy]        # define the lint policy once, not per crate
unwrap_used = "deny"

[profile.release]               # profiles ONLY take effect at the workspace root
lto = "fat"
codegen-units = 1
```
Members inherit with `edition.workspace = true`, `serde = { workspace = true }`, and `[lints] workspace = true`. One `Cargo.lock`, one `target/` shared across members. See `references/build-and-deps.md` for the full three-table centralization pattern, the exclusivity rule on `[lints] workspace = true`, and the profile set.

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

Next: to turn a boundary into a pattern (sealed trait, typestate), load `references/design-patterns.md`; for the build side of a workspace split, `references/build-and-deps.md`.
