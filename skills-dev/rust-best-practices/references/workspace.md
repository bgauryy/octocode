Load when you add a crate to a workspace, map a Node monorepo habit to Cargo, write or review a workspace root `Cargo.toml`, centralize versions/lints, or audit feature and version hygiene. Why: the root manifest centralizes versions, lints, and features that otherwise drift per crate. Whether and where to split: `references/crate-structure.md`. Profiles: `references/build-profiles.md`.

# Workspace: crates and root manifest

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
- A flat `crates/*` layout makes members easy to find. Keep a nested layout when existing tooling or domain grouping benefits from it; Cargo member paths are explicit.
- A virtual root sets its resolver explicitly; it has no package edition to infer from. Edition 2024 implies resolver `"3"` only on a package root. A new virtual workspace can set `"3"`. Preserve an existing choice unless dependency resolution is the task; Octocode currently uses `"3"`. Changing it affects dependency resolution. [Cargo resolver versions](https://doc.rust-lang.org/cargo/reference/resolver.html#resolver-versions).
- One `Cargo.lock`, one `target/`.
- Add one: `cargo new --lib crates/app-fs` (auto-joins `members`), `cargo add -p app-cli --path crates/app-fs`, then move the dep into `[workspace.dependencies]`.

## Root manifest
```toml
[workspace.package]                 # shared metadata
edition = "2024"                    # members may inherit this during a staged migration
rust-version = "1.89"               # single MSRV floor
license = "MIT"
repository = "https://github.com/org/repo"

[workspace.dependencies]            # one version + baseline features per dep
serde = { version = "1", features = ["derive"] }
tokio = { version = "1" }           # members add features on top
napi = { version = "3", default-features = false, features = ["napi4"] }

[workspace.lints.rust]
warnings = { level = "deny", priority = -1 }  # cargo check fails rustc warnings
unused_imports = "deny"
let_underscore_drop = "allow"                 # default priority overrides `warnings`
[workspace.lints.clippy]
all = { level = "deny", priority = -1 }   # group at low priority; single lints above win
unwrap_used = "deny"
expect_used = "deny"
panic = "deny"
todo = "deny"
unimplemented = "deny"
dbg_macro = "deny"
```
Members: `edition.workspace = true`, `serde = { workspace = true }`, `tokio = { workspace = true, features = ["fs"] }`, `napi = { workspace = true, optional = true }` (`optional` is per member), `[lints] workspace = true`.

- **`[workspace.dependencies]`** centralizes a shared version and baseline features; this matters when crates must agree on a dependency's types or ABI. Centralize dependencies used by multiple crates or across an ABI boundary; single-use dependencies can stay local.
- **`[workspace.lints]`**: duplicated lint blocks drift. `[lints] workspace = true` cannot add extras in the manifest; a stricter crate keeps its own `[lints]` block. A single `#![deny(some_lint)]` belongs on one module. Do not put `#![deny(warnings)]` at a crate root: the next rustc warning fails that build, including for a published crate's own CI. Cargo caps lints from dependencies, so the attribute does not police downstream. The manifest floor is the replacement ([deny warnings](https://rust-unofficial.github.io/patterns/anti_patterns/deny-warnings.html)).
- **The floor lives in the manifest.** `warnings = { level = "deny", priority = -1 }` makes `cargo check` and rust-analyzer fail on rustc warnings. A `-D warnings` that exists only on a yarn or CI script does not. Clippy lints still run only under `cargo clippy`. The editor stays on `cargo check` (`references/testing-and-tooling.md`).
- **Group priority.** A group at `priority = -1` lets one lint at the default priority override it. Put `clippy::all` and `warnings` at `-1`. An intentional allow such as `let_underscore_drop` stays at the default priority. [Cargo lints](https://doc.rust-lang.org/cargo/reference/manifest.html#the-lints-section).
- **Groups to leave off.** Do not enable `clippy::restriction` as a group: the lints contradict each other, and Clippy ships `blanket_clippy_restriction_lints` against that. Do not deny `clippy::pedantic` or `clippy::nursery` for the whole workspace; pedantic has known false positives ([Clippy](https://doc.rust-lang.org/clippy/)). Deny one lint from those groups on the crate that needs it (`references/safety-and-ffi.md`). `clippy::cargo` asks internal `publish = false` crates for registry metadata; enable one cargo lint, not the group.
- A library that prints is a bug. Deny `clippy::print_stdout` and `clippy::print_stderr` on the library crate. Leave them off the CLI binary.
- **Edition migration.** A shared edition simplifies workspace conventions, but members may use different editions during a staged migration. Edition-specific rules apply within each crate; check cross-crate macro and API behavior, run `cargo fix --edition`, and verify before changing a member's edition.
- `version` may live in `[workspace.package]` only if release tooling reads it there; regex scripts that rewrite per-crate `version = "…"` break on `version.workspace = true`.
- A `rust-toolchain.toml` channel above the `rust-version` floor is normal: toolchain = what CI builds with; MSRV = the compatibility promise (enforce with `clippy::incompatible_msrv` or `cargo msrv verify`).
- In Octocode, inherit version and metadata from `[workspace.package]`; internal Cargo crates stay `publish = false`. Extracting a Rust crate does not create an npm package.

## Split mechanics and hygiene
- **Crates are not free**: each adds a link/metadata step and blocks cross-crate inlining without `#[inline]`/LTO. Tens of meaningful crates beat hundreds of 50-line ones; a file-sized concern stays a module.
- **Features unify per command, not per workspace**: one member enabling `tokio/full` turns it on for that build only. A different `-p` selection, `--all-features`, or dev-dependency features (test builds) can produce separate compiled copies. Align selections when compile cost matters; an opt-in `dev-unify` feature is one measured approach (`references/build-profiles.md`). Vet generated feature unification against shipped builds.
- Heavy deps: `default-features = false` + only what you use (smaller graph, faster builds, less attack surface).
- Publishing: internal path deps also need `version = "x.y"`; publish leaves first (release-plz orders it).
- Commit `Cargo.lock` for apps and libraries (current Cargo guidance); CI builds with `--locked`; never hand-edit it.

## Verify
`cargo check --workspace --all-targets` · `cargo tree -p <crate> -e normal` (heavy dep dropped?) · `cargo tree -d` (duplicates) · `cargo build --timings` (critical path shorter?) · unused-dependency scan (`references/testing-and-tooling.md`).
