---
name: rust-best-practices
description: "Load this skill first when writing, reviewing or restructuring Rust in packages/octocode-native: where code belongs, the lints and ratchet bans it must pass, test layout and the build settings measured here."
---

# Rust in octocode-native

One Cargo workspace in `packages/octocode-native` (paths below are relative to it): 17 crates, edition 2024, resolver 3, toolchain and MSRV 1.99.1 (`rust-toolchain.toml`). [ARCHITECTURE.md](../../packages/octocode-native/ARCHITECTURE.md) owns crate roles; `CRATE_EDGES` in `crates/runtime/tests/architecture_guards.rs` enforces the allowed edges. Folders drop the `octocode-` prefix, except `crates/runtime`, which is the `octocode-native` facade.

## Where code goes

- A tool's behavior lives in its family crate (`crates/tool-*`), code two tools share in `tools-shared` (layers: `SHARED_LAYERS` in `crates/runtime/src/tools/module_boundaries/tool_edges.rs`), the layers below the tools in `runtime-base`. The facade keeps admission, dispatch and wiring only: every edit below it recompiles it and the CLI.
- A family never imports or dev-depends on another family. Build-wide values (build identity, clocks, config) are parameters passed down, never globals.
- Cross-crate primitives have one owner, `crates/io` (`octocode_io`): atomic writes, locks, env reads, HTTP clients, `cancellation`, the request `budget`, retries, bounded reads, `notice`. Child processes go through the engine's `process_env` and `tools-shared`'s `bounded_process`.
- Many functions still return `Result<_, String>`: add none at a crate or module boundary, and type one when you change its callers anyway.

## Lints that fail the build

- Workspace `[lints]` (root `Cargo.toml`) deny `dead_code`, `unused_*`, `private_interfaces`, `private_bounds`, `clippy::all`, `unwrap_used`, `expect_used`, `panic`, `todo`, `dbg_macro` and `undocumented_unsafe_blocks`; `unreachable_pub` and `mod_module_files` (`name.rs` plus `name/`, never `mod.rs`) warn, and CI denies warnings. Library `lib.rs` headers also deny prints (use `octocode_io::notice`) and warns on lossy `as` casts outside tests (use `try_from`).
- Suppress only with `#[expect(lint, reason = "…")]` on the smallest item. `clippy.toml` already lets tests unwrap, expect, panic and print.
- `$DEV lint:rust` adds production-only bans (`clippy-ratchet.toml`): `Duration::from_secs`/`from_millis`, `thread::sleep`, `tokio::time::{sleep, timeout}`, `Vec`/`String::truncate`, `Command::new`, `reqwest::Client` construction, `fs::read`, `fs::read_to_string`, `fs::write`, `File::create`. Older findings sit per crate in `clippy-baseline.json` and may only shrink. A site that must stay: `#[cfg_attr(octocode_clippy_ratchet, expect(clippy::disallowed_methods, reason = "…"))]`.

## Tests

- Unit tests: `#[cfg(test)] mod tests;` beside the code with the body in its own file, never inline. The build identity (`crates/runtime/build/`) hashes only non-test files, so a test edit in its own file recompiles one unit; inline, it rebuilds the facade and the CLI.
- Integration tests: one binary per crate (`autotests = false`, `tests/main.rs`). A new `tests/foo.rs` compiles only once `main.rs` declares `mod foo;`.
- Fixtures: the dev-only `crates/test-support` when they need no internals, else the owning crate's `test-support` feature.
- One test selection (`scripts/rust-tests.cjs`, repeated by the `cargo dev-test` alias and `bacon.toml`). A new dev-dependency feature joins `dev-unify` (`crates/runtime/Cargo.toml`) in the same change, and no `[profile.test]` setting may differ from dev: either forks every crate between `build:dev` and the tests. Inject clocks; never tokio's `test-util`.
- A test that starts a real child process joins `PROCESS_TESTS` in `rust-tests.cjs` (nextest); everything else runs under libtest.

## Commands beyond AGENTS.md

`$DEV check` (rustfmt, ratchet and lints, no build), `$DEV docs:rust`, `$DEV llvm-lines [package] [--timings]`, `$DEV profile <tool> '<json>'`, `$DEV miri`, `$DEV fuzz:smoke`, `$DEV mutants -f <file>`, `$DEV rust:tools` (pins in `dev-tools.json`). In `packages/octocode-native`: `yarn deny`, `yarn shear`, `yarn check:features`, `yarn coverage:rust`.

## Branches

- Adding a crate, splitting one or moving code between crates: [crate structure](references/crate-structure.md).
- Compile time, needless rebuilds, profile settings or the cost of generic and generated code: [build profiles](references/build-profiles.md).
