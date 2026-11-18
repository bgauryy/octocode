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
- Under resolver 2, dev-dependency features apply only to commands that build tests, examples or benches. A test-only feature of a shared dependency, such as tokio's `test-util`, therefore gives that dependency and every crate above it a second copy, and each switch between `cargo build` and `cargo test` recompiles them. Measured here: with one such feature missing from the dev build, a test build after a dev build compiled 27 units, every library included, instead of 5 test units.
- Remove the fork at its cause, in this order. First, have tests stop needing the feature: inject a clock instead of tokio's paused time ([testing](testing-and-tooling.md#write-trustworthy-tests)). Second, move fixtures that need no crate internals into a dev-only crate ([crate structure](crate-structure.md#clean-crate-architecture)). Only then have the dev build enable the remaining set through an opt-in feature that release builds never select. That feature compiles whatever it enables into every dev binary, so it holds dependency features, not test stubs.
- Prove the dev build and the test selection share their non-test units. Diff the unit keys (package, target, mode, features, profile and, recursively, dependencies) of `cargo +nightly build --unit-graph -Z unstable-options` and the same for `cargo test`, with identical package and feature selections. Expect 0 dev units missing from the test graph, and check the release graph for test features too.
- Remove a feature fork at its cause instead of adding a workspace-hack crate (cargo-hakari). Materialize removed theirs because it "slows full-workspace builds by ~6% and cold single-crate builds by ~20%" ([PR #35718](https://github.com/MaterializeInc/materialize/pull/35718)), and Zed removed its own ([PR #40216](https://github.com/zed-industries/zed/pull/40216)).
- Test the promised MSRV using the supported package/target/features. Prefer existing CI or toolchain runners. A modern compiler plus `clippy::incompatible_msrv` is useful feedback, not an MSRV build.
- Before using a newly stabilized API, verify its stabilization version as well as its edition requirements. Do not raise MSRV, update every dependency or regenerate an unrelated lockfile just to silence a diagnostic.
- Treat edition/resolver changes as migrations: include callers/macros, behavior-sensitive scope changes and relevant tests. `cargo fix --edition` is an explicit mutation, not a routine verification command.

## Dependency hygiene

Use `cargo metadata --format-version 1 --no-deps` for package/target relationships and `cargo tree` for the resolved graph. Unused-dependency scanners ([tools](testing-and-tooling.md#tools-that-keep-code-and-architecture-clean)) supply candidates; inspect build scripts, proc macros, generated code, renamed dependencies and feature-gated uses before removal. `--fix` and metadata-assisted scans can mutate manifests/lockfiles; inspect their contract first.

Use existing task automation. Avoid introducing a new task runner or crate merely to mirror one Cargo command. For release packaging, use the project's validation path; publishing is a separate authorized action.

Sources: [Cargo workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html), [features](https://doc.rust-lang.org/cargo/reference/features.html), [resolver 2 dev-dependency features](https://doc.rust-lang.org/cargo/reference/features.html#feature-resolver-version-2), [unit graph](https://doc.rust-lang.org/cargo/reference/unstable.html#unit-graph), [rust-version](https://doc.rust-lang.org/cargo/reference/rust-version.html), [lints](https://doc.rust-lang.org/cargo/reference/manifest.html#the-lints-section), [Cargo.lock](https://doc.rust-lang.org/cargo/guide/cargo-toml-vs-cargo-lock.html).
