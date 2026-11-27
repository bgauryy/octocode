---
name: rust-best-practices
description: "Load this skill first when writing, reviewing or restructuring Rust in packages/octocode-native: where code belongs, the lints and ratchet bans it must pass, test layout and the build settings measured here."
---

# Rust in octocode-native

One Cargo workspace in `packages/octocode-native` (paths are relative to it): 17 crates, edition 2024, resolver 3, toolchain = MSRV (`rust-toolchain.toml`). [ARCHITECTURE.md](../../packages/octocode-native/ARCHITECTURE.md) owns crate roles; `CRATE_EDGES` in `crates/runtime/tests/architecture_guards.rs` enforces the allowed edges. Folders drop the `octocode-` prefix; `crates/runtime` is the `octocode-native` facade. `$DEV` is the [AGENTS.md](../../AGENTS.md#work-locally) task runner. Check a change against each section; return the [review result](#review-result). Adding, splitting or moving crates: [crate structure](references/crate-structure.md); compile time, rebuilds or profiles: [build profiles](references/build-profiles.md).

## Where code goes

- A tool's behavior lives in its family crate (`crates/tool-*`), code two tools share in `tools-shared` (layers: `SHARED_LAYERS`), the layers below the tools in `runtime-base`. The facade keeps admission, dispatch and wiring only: every edit below it recompiles it and the CLI.
- A family never imports or dev-depends on another family. Pass build-wide values (build identity, clocks, config) down as parameters, never globals.
- `crates/io` (`octocode_io`) solely owns cross-crate primitives: atomic writes, locks, env reads, HTTP clients, `cancellation`, the request `budget`, retries, bounded reads, `notice`. Child processes go through the engine's `process_env` and `tools-shared`'s `bounded_process`.
- Add no `Result<_, String>` at a crate or module boundary; type an existing one when you change its callers anyway.

## Lints that fail the build

- Workspace `[lints]` (root `Cargo.toml`) deny dead and unused code, private interfaces and bounds, `clippy::all`, `unwrap`, `expect`, `panic!`, `todo!`, `dbg!` and undocumented `unsafe`; `unreachable_pub` and `mod_module_files` (`name.rs` plus `name/`, never `mod.rs`) warn, and CI denies warnings. Library `lib.rs` headers also deny prints (use `octocode_io::notice`) and warn on lossy `as` casts outside tests (use `try_from`).
- Suppress only with `#[expect(lint, reason = "…")]` on the smallest item. `clippy.toml` already lets tests unwrap, expect, panic and print.
- `$DEV lint:rust` adds production-only bans from `clippy-ratchet.toml`, each naming its replacement: fixed durations, sleeps and timeouts, `truncate`, `Command::new`, `reqwest` client construction, unbounded `fs` reads and non-atomic writes. Older findings sit per crate in `clippy-baseline.json`, which only shrinks. A site that must stay takes the `cfg_attr(octocode_clippy_ratchet, expect(…))` from the toml header.

## Tests

- Unit tests: `#[cfg(test)] mod tests;` beside the code with the body in its own file, never inline (an inline edit rebuilds the facade and the CLI).
- Integration tests: one binary per crate (`autotests = false`, `tests/main.rs`); a new `tests/foo.rs` needs `mod foo;` in `main.rs`.
- Fixtures: the dev-only `crates/test-support` when they need no internals, else the owning crate's `test-support` feature.
- One test selection (`scripts/rust-tests.cjs`, mirrored in the `dev-test` alias and `bacon.toml`). A new dev-dependency feature joins `dev-unify` (`crates/runtime/Cargo.toml`) in the same change, and `[profile.test]` matches dev ([rebuild causes](references/build-profiles.md#rebuild-causes-found-here)). Inject clocks; never tokio's `test-util`.
- A test that starts a real child process joins `PROCESS_TESTS` in `rust-tests.cjs` (nextest; the rest run under libtest).

## Commands beyond AGENTS.md

`$DEV check` (rustfmt, ratchet and lints, no build); `$DEV --help` gives arguments for `docs:rust`, `llvm-lines`, `profile`, `miri`, `fuzz:smoke`, `mutants`, `rust:tools`. In `packages/octocode-native`: `yarn deny`, `yarn shear`, `yarn check:features`, `yarn coverage:rust`.

## Review result

Stop when `$DEV check`, `$DEV lint:rust` and focused `$DEV test:dev --crate <crate> -- <filter>` pass, plus `--crate native -- architecture_guards` when crate edges move. Return the verdict (pass, or the first failing check at `file:line`), then new `#[expect]`s with reasons, the baseline equal or smaller, each new test's place and result, and crate edges changed.
