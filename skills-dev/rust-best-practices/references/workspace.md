# Workspace: crates and root manifest

Load when you add a crate to a workspace, map a Node monorepo habit to Cargo, write or review a workspace root `Cargo.toml`, centralize versions/lints, or audit feature and version hygiene. Whether and where to split: `references/crate-structure.md`. Profiles: `references/build-profiles.md`.

## npm → Cargo map
| Node / npm | Cargo |
|---|---|
| `package.json` · `private: true` | `Cargo.toml` · `publish = false` |
| `workspaces: ["packages/*"]` | root `[workspace] members = ["crates/*"]` (virtual manifest, no `[package]`) |
| `"dep": "workspace:*"` | `[workspace.dependencies] my-core = { path = "crates/my-core" }` → member `my-core.workspace = true` |
| `exports` / `index.ts` barrel | `lib.rs` + `pub use` |
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
- Flat `crates/*`; folder name == crate name (kebab in Cargo, snake in `use`). Do not nest crates.
- The virtual root sets `resolver = "3"` explicitly (edition 2024; `"2"` on 2021): it has no edition to infer from.
- One `Cargo.lock`, one `target/`.
- Add one: `cargo new --lib crates/app-fs` (auto-joins `members`), `cargo add -p app-cli --path crates/app-fs`, then move the dep into `[workspace.dependencies]`.

## Root manifest
```toml
[workspace.package]                 # shared metadata
edition = "2024"                    # ONE edition for the whole workspace
rust-version = "1.89"               # single MSRV floor
license = "MIT"
repository = "https://github.com/org/repo"

[workspace.dependencies]            # one version + baseline features per dep
serde = { version = "1", features = ["derive"] }
tokio = { version = "1" }           # members add features on top
napi = { version = "3", default-features = false, features = ["napi4"] }

[workspace.lints.rust]
unused_imports = "deny"
[workspace.lints.clippy]
all = { level = "deny", priority = -1 }   # enable group at low priority, override individuals above
unwrap_used = "deny"
```
Members: `edition.workspace = true`, `serde = { workspace = true }`, `tokio = { workspace = true, features = ["fs"] }`, `napi = { workspace = true, optional = true }` (`optional` is per member), `[lints] workspace = true`.

- **`[workspace.dependencies]`** prevents version skew on shared ABIs: two crates linking one N-API addon must compile the same `napi`/`napi-derive`. Centralize deps used by ≥2 crates or ABI/security-critical; single-use deps can stay local.
- **`[workspace.lints]`**: duplicated lint blocks drift. `[lints] workspace = true` cannot add extras in the manifest; a stricter crate keeps its own `[lints]` block or adds crate-root `#![deny(...)]`.
- **One edition.** Mixed 2021/2024 means different macro hygiene, lifetime capture, and temporary scopes between crates that call each other. Migrate with `cargo fix --edition` + verify; do not just flip the string.
- `version` may live in `[workspace.package]` only if release tooling reads it there; regex scripts that rewrite per-crate `version = "…"` break on `version.workspace = true`.
- A `rust-toolchain.toml` channel above the `rust-version` floor is normal: toolchain = what CI builds with; MSRV = the compatibility promise (enforce with `clippy::incompatible_msrv` or `cargo msrv verify`).

## Split mechanics and hygiene
- **Crates are not free**: each adds a link/metadata step and blocks cross-crate inlining without `#[inline]`/LTO. Tens of meaningful crates beat hundreds of 50-line ones; a file-sized concern stays a module.
- **Features unify across a combined build**: one member enabling `tokio/full` turns it on for all; `cargo-hakari` stops rebuild churn when members differ.
- Heavy deps: `default-features = false` + only what you use (smaller graph, faster builds, less attack surface).
- Publishing: internal path deps also need `version = "x.y"`; publish leaves first (release-plz orders it).
- Commit `Cargo.lock` for apps and libraries (current Cargo guidance); CI builds with `--locked`; never hand-edit it.

## Verify
`cargo check --workspace --all-targets` · `cargo tree -p <crate> -e normal` (heavy dep dropped?) · `cargo tree -d` (duplicates) · `cargo build --timings` (critical path shorter?) · `cargo machete` (stale deps).
