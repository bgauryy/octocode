# Cargo workspaces, features and compatibility

Load for manifests, dependency resolution, features, lockfiles, MSRV or edition changes. Inspect the actual workspace root, member manifests, Cargo config and CI before editing. Cargo package names need not match directory names.

## Establish the contract

| Setting | Meaning | Check |
|---|---|---|
| `edition` | Language edition for each package | Resolve `edition.workspace = true`; members may differ. |
| `rust-version` | Minimum supported Rust version | Resolve inheritance; check the stated target/feature support policy. The installed toolchain is not the MSRV. |
| `rust-toolchain.toml` | Toolchain, components and targets selected for development | Preserve pins; install/change only within authorized scope. |
| `resolver` | Dependency feature/version resolution behavior | A virtual root cannot infer it from a package edition. Preserve the existing resolver unless migration is intended. |
| `Cargo.lock` | Resolved versions for this workspace/build | Let Cargo maintain it; review resulting changes. `--locked` refuses an update. |
| `publish` and versions | Release policy | Internal crates need not share a version unless project tooling requires it. |

Current Cargo guidance recommends committing the lockfile for applications and libraries; follow the repository's declared policy. A library's checked-in lockfile does not pin its downstream consumers' dependency graph. Keep version requirements and MSRV support honest independently of that one resolution.

## Shared manifests

- Use `[workspace.dependencies]` for shared version/feature policy and `[workspace.package]` for metadata that members deliberately inherit. Put `optional = true` on the member dependency. Inherited features are additive.
- Configure shared lints under `[workspace.lints]`; a member opts in with `[lints] workspace = true`. That member cannot simultaneously add its own manifest lint table; use an appropriate item-level lint or a separate member policy.
- Put lint groups at lower priority than intended per-lint overrides. Do not copy Octocode's lint floor into unrelated projects or weaken an existing floor to pass a change.
- Denying all warnings is a project choice with toolchain-upgrade costs, wherever configured. Use CI `-D warnings` when that is the established gate. Never enable all `clippy::restriction` lints as a group; select justified lints and review diagnostics.
- Features of a dependency unify for the command's selected graph according to the resolver. Use `cargo tree -e features -i <dependency>` to find who enables a feature; setting `default-features = false` on one edge cannot disable another edge's enablement.

## Features and MSRV

- Design library features to be additive: enabling one should not remove functionality or break consumers. `std` as an opt-in capability usually composes better than a feature that removes std.
- Inspect documented mutually exclusive backends and platform constraints before running `--all-features`. Test supported combinations; do not rewrite a legitimate backend policy merely to make that flag compile.
- Test default, minimal supported and affected feature combinations. One workspace build can hide a missing feature dependency through unification; check the relevant package on its own when that risk exists.
- Test the promised MSRV using the supported package/target/features. Prefer existing CI or toolchain runners. A modern compiler plus `clippy::incompatible_msrv` is useful feedback, not an MSRV build.
- Before using a newly stabilized API, verify its stabilization version as well as its edition requirements. Do not raise MSRV, update every dependency or regenerate an unrelated lockfile just to silence a diagnostic.
- Treat edition/resolver changes as migrations: include callers/macros, behavior-sensitive scope changes and relevant tests. `cargo fix --edition` is an explicit mutation, not a routine verification command.

## Dependency hygiene

Use `cargo metadata --format-version 1 --no-deps` for package/target relationships and `cargo tree` for the resolved graph. Unused-dependency scanners supply candidates; inspect build scripts, proc macros, generated code, renamed dependencies and feature-gated uses before removal. `--fix` and metadata-assisted scans can mutate manifests/lockfiles; inspect their contract first.

Use existing task automation. Avoid introducing a new task runner or crate merely to mirror one Cargo command. For release packaging, use the project's validation path; publishing is a separate authorized action.

Sources: [Cargo workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html), [features](https://doc.rust-lang.org/cargo/reference/features.html), [rust-version](https://doc.rust-lang.org/cargo/reference/rust-version.html), [lints](https://doc.rust-lang.org/cargo/reference/manifest.html#the-lints-section), [Cargo.lock](https://doc.rust-lang.org/cargo/guide/cargo-toml-vs-cargo-lock.html).
