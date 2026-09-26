# Workspace crates — split code like npm packages

Load when splitting a crate into small internal crates, adding a crate to a workspace, or mapping a Node monorepo habit to Cargo. Why: a crate is Rust's package *and* compilation unit — small focused crates give npm-style isolation plus parallel, cached builds; wrong splits give cycles, feature bloat, and version skew.

## npm → Cargo map
| Node / npm | Cargo |
|---|---|
| `package.json` · `private: true` | `Cargo.toml` · `publish = false` |
| `workspaces: ["packages/*"]` | root `[workspace] members = ["crates/*"]` (virtual manifest, no `[package]`) |
| `"dep": "workspace:*"` | `[workspace.dependencies] my-core = { path = "crates/my-core" }` → member `my-core.workspace = true` |
| `exports` / `index.ts` barrel | `lib.rs` + `pub use`; everything else `pub(crate)` |
| `devDependencies` · `optionalDependencies` | `[dev-dependencies]` · `optional = true` + `[features]` |
| `engines.node` | `rust-version` (MSRV) |
| `package-lock.json` · `npm ci` | `Cargo.lock` · `cargo build --locked` |
| `npm run x` / scripts | `.cargo/config.toml` `[alias]`, or an `xtask` crate for real logic |
| `npm -w pkg run test` | `cargo test -p pkg` (`--workspace`, `--exclude pkg`) |
| `patch-package` / `overrides` | `[patch.crates-io]` |
| changesets | `release-plz` (or `cargo-release`) + `cargo-semver-checks` |

## Layout (flat, matklad's "Large Rust Workspaces")
```text
Cargo.toml            # virtual manifest: [workspace] + shared package/deps/lints/profiles
crates/
  app-cli/            # thin binary: args → lib → exit code
  app-core/           # pure domain, no I/O
  app-fs/  app-http/  # adapters, one I/O concern each
xtask/                # dev automation in Rust (codegen, release checks), publish = false
```
- Flat `crates/*`, folder name == crate name (kebab in Cargo, snake in `use`). Don't nest crates inside crates.
- Virtual root sets `resolver = "3"` explicitly (edition 2024; `"2"` on 2021) — a virtual manifest has no edition to infer it from.
- Add one: `cargo new --lib crates/app-fs` (auto-joins `members`), then `cargo add -p app-cli --path crates/app-fs`, and move the dep into `[workspace.dependencies]`.
- Inherit everything shared: `edition.workspace = true`, `[lints] workspace = true`, deps `{ workspace = true }` — see `references/workspace-manifest.md`.

## Split mechanics
- *Whether* to split and the boundary rules: `references/crate-boundaries.md`.
- **Dependencies point inward, no cycles** — Cargo forbids crate cycles; a would-be cycle means a missing lower crate or a trait the upper layer implements.
- **Crates are not free:** each adds a link/metadata step and blocks cross-crate inlining without `#[inline]`/LTO. Tens of meaningful crates beat hundreds of 50-line ones; a file-sized concern stays a module.
- **Features unify across the workspace build** — one member enabling `tokio/full` turns it on for all in a combined build. Keep features additive; check with `cargo hack check --each-feature -p <crate>`; `cargo-hakari` stops rebuild churn when members differ.
- Keep test fixtures/helpers in a `publish = false` `*-test-support` crate used only as a dev-dependency.
- Publishing: internal path deps also need `version = "x.y"`; publish leaves first (release-plz orders it).

## Verify
`cargo check --workspace --all-targets` · `cargo tree -p <crate> -e normal` (did the split drop the heavy dep?) · `cargo build --timings` (did the critical path shrink?) · `cargo machete` (stale deps left behind).

Next: for what goes in which crate and the API between them, load `references/crate-boundaries.md`; for module/file layout inside each crate, `references/project-structure.md`; for shared deps/lints, `references/workspace-manifest.md`.
