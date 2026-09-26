# Workspace manifest — shared metadata, dependencies, lints, versions

Load when writing or reviewing a workspace root `Cargo.toml`, centralizing versions/lints, or auditing feature and version hygiene. Why: the cohort (oxc, ruff, uv, biome, rolldown, rspack, deno) centralizes three tables at the root; missing one is a drift hazard, not a style choice.

## The three tables
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

## Why each matters
- **`[workspace.dependencies]`** prevents *version skew on shared ABIs* — two crates linking one N-API addon must compile the same `napi`/`napi-derive`. Centralize deps used by ≥2 crates or ABI/security-critical; single-use deps can stay local.
- **`[workspace.lints]`** — duplicated lint blocks drift. Exclusivity: `[lints] workspace = true` can't add extras in the manifest; a stricter crate keeps its own `[lints]` block or adds crate-root `#![deny(...)]`.
- **One edition** — mixed 2021/2024 means different macro hygiene, lifetime capture, and temporary scopes between crates that call each other. Migrate with `cargo fix --edition` + verify; don't just flip the string.
- `version` may live in `[workspace.package]` only if release tooling reads it there — regex scripts that rewrite per-crate `version = "…"` break on `version.workspace = true`.
- A newer `rust-toolchain.toml` channel above the `rust-version` floor is normal: toolchain = what CI builds with; MSRV = the compatibility promise (enforce with `clippy::incompatible_msrv` or `cargo msrv verify`).

## Feature & version hygiene
- Heavy deps with `default-features = false` + only what you use — smaller graph, faster builds, less attack surface.
- Commit `Cargo.lock` for apps and for libraries too (current Cargo guidance); build CI with `--locked`; never hand-edit it.
- Watch duplicates with `cargo tree -d`; unused deps with `cargo machete`; the audit gate lives in `references/safety-and-security.md`.

Adding/bumping deps, changing MSRV/edition, or touching the lockfile needs consent (SKILL lobby).

Next: for profiles and compile time, load `references/build-profiles.md`; for the crate layout the manifest describes, `references/workspace-crates.md`.
