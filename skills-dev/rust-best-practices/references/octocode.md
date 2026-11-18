# Octocode-specific Rust development

Load only inside the Octocode monorepo. Read the checkout's `AGENTS.md` and `skills-dev/octocode-dev/SKILL.md` first. Those live owners override examples here; do not copy these project policies into unrelated Rust repositories.

Paths below are relative to the repository root. They are source locations to inspect in that checkout, not dependencies required in a standalone skill bundle.

## Ownership

| Area | Source owner to inspect |
|---|---|
| Crate responsibilities and dependency direction | `packages/octocode-native/ARCHITECTURE.md`; `skills-dev/octocode-dev/docs/DEVELOPMENT.md`; the allowed crate edges in `CRATE_EDGES` (`packages/octocode-native/crates/runtime/tests/architecture_guards.rs`) and the module layers in the runtime's `module_boundaries` guards |
| Public schemas, descriptions, instructions and limits | Sibling core `../octocode-mcp-host/packages/octocode-core`; `@octocodeai/config` generates `packages/octocode-config/contract/` |
| Execution, admission, security, errors and response shaping | Native runtime; reusable algorithms in engine; CLI/MCP and Node launchers adapt/delegate |
| Config/home/env resolution | `@octocodeai/config` rules; do not duplicate loading in tool/host adapters |
| Toolchain, features, lint/profile settings and tool pins | Native `Cargo.toml`, `rust-toolchain.toml`, `dev-tools.json`; task scripts are authoritative |
| Shared skill resources | `skills/common`; generated copies must be regenerated, not hand-edited |

For contract changes: build core, run `node skills-dev/octocode-dev/scripts/dev.mjs contracts:regen`, verify the changed text in generated output, rebuild native and affected consumers, then restart the MCP server used for verification. Use generated wire types. Declare new field effects or confirm documented ancestor inheritance; the declaration is not a behavioral test.

## Crate layout

- The runtime is being split along its enforced layers, following the [crate architecture rules](crate-structure.md#clean-crate-architecture). `octocode-runtime-base` holds the layers below the tools, and `contract-base` the few generated types `github` names. The planned next steps are a `tools-shared` crate, one crate per tool family, a dispatch-only facade (`octocode-native`) and a dev-only `octocode-test-support` crate. List `packages/octocode-native/crates/` for what has landed. A new crate updates `CRATE_EDGES`, the test selection, the release `codegen-units` override, the dev `debug = false` override, the Clippy baseline keys and the crate docs in the same change.
- The private root `use octocode_runtime_base::{…}` and the `mod response` shim in `crates/runtime/src/lib.rs` are temporary bridges. Do not add users; when you touch a file that uses one, rewrite its paths to `octocode_runtime_base::…`.
- The build identity is a parameter (`ResponsePageOptions::build_identity`) that the top crate passes down, never a global.
- Tests stay out of implementation files (owner rule): a test module is its own file, mounted by `#[cfg(test)] mod name;`, and registered only after the file exists.

## Development and checks

- Use `node skills-dev/octocode-dev/scripts/dev.mjs <task>` from the root. `build:dev` targets the host for local work; use native release/profiling builds for production performance measurements. Cross-platform/release builds require that task scope.
- Build, test and lint Rust only through the wrappers `AGENTS.md` names. Each takes one of the machine's two build slots (`packages/octocode-native/scripts/build-slot.cjs`) and gives its child cores ÷ slots for `CARGO_BUILD_JOBS` and the test threads; a bare `cargo` call is wrapped as `node packages/octocode-native/scripts/build-slot.cjs -- cargo …`. `AGENTS.md` owns the limit, retries and cleanup. One build owner uses the shared repository `target/`; focused fixers run focused tests. Do not create a private target directory or an isolated copy to evade contention, clean shared artifacts or format the whole tree during a concurrent build.
- There is one test selection: every workspace crate with `octocode-native/dev-unify`, held by `packages/octocode-native/scripts/rust-tests.cjs` and repeated by `cargo dev-test` and `bacon.toml` (a script test keeps them equal). Any other selection, such as a bare `cargo test -p <crate>`, compiles a second copy of the workspace. `test:rust` runs the selection under libtest, behind a watchdog, and only the tests that spawn real processes (`PROCESS_TESTS`) under nextest; the reason is written in the `rust-tests.cjs` header. Check manifest doctest settings rather than assuming a runner covered examples.
- `dev-unify` (`packages/octocode-native/crates/runtime/Cargo.toml`) gives `build:dev` the features the test selection's dev-dependencies enable, so the build and the tests share every non-test unit. Add a new or changed dev-dependency feature to it in the same change and prove the sharing with the unit-graph diff in [workspace](workspace.md#features-and-msrv). The aim is for it to carry no test code: fixtures move to the test-support crate, and injected values replace the engine's `test-hooks` and tokio's `test-util`. Do not add a `[profile.test]` setting that differs from dev: `cargo test` builds every unit under that profile, so it forks them all.
- The runtime build identity (`packages/octocode-native/crates/runtime/build/`) hashes and watches, file by file, only what a non-test build compiles, skipping `#[cfg(test)]` module subtrees, so a unit-test edit recompiles one unit. Keep it so: never watch a directory or a path that may not exist ([needless rebuilds](build-profiles.md#stop-needless-rebuilds)).
- Dev profile: each large workspace crate gets `[profile.dev.package.<crate>] debug = false`; `codegen-units` and dependency opt-levels change only with an A/B whose numbers go in the `Cargo.toml` comment beside them ([profile decisions](build-profiles.md#profile-decisions)).
- Many native functions still return `Result<_, String>`. Add none at a crate or module boundary, and give one a typed error when you change its callers anyway ([idioms](idioms.md#errors-and-panics)).
- `lint:rust` owns the Clippy ratchet (`skills-dev/octocode-dev/scripts/rust-lint.mjs`, `packages/octocode-native/clippy-baseline.json`). Inspect its source/config before changing suppressions or baselines; floors and baseline counts must not grow to pass a change. `deny`, `shear` and `check:features` are the other dependency gates. Tool versions and optional verification selections live in `dev-tools.json`.
- After native behavior changes, rebuild the affected interface and exercise CLI config/schema plus the changed call; use a fresh MCP process. Check each result row, errors and warnings rather than relying on the envelope exit status.
- For skills/prose changes, run `skills:check` and applicable documentation verification. A prose-only change does not justify a native rebuild.

## Evidence and lifetime invariants

- Preserve all selected tool evidence. Walk response pages before tool-level continuations; verify reconstruction exactly once without gaps/duplicates. Keep indivisible values whole and disclose terminal limits/unread scope.
- Do not add guessed timeouts, match/file caps or truncation as a resource fix. Follow the owning contract/resource policy and return a continuation or explicit refusal.
- Preserve request registration, cancellation, blocking-worker completion and shutdown invariants. Test admission failure, pre-start drop, cancellation while running and close with outstanding resources.
- Native launchers/hosts must not gain fallback tool implementations. Match CLI and MCP schemas, fields and observable behavior.

Do not commit or stash in this shared checkout; leave source changes for the owner. Stop only processes started for this task, by PID/task ID. Remove task scratch and private artifacts before handoff, preserving cited evidence and other sessions' changes.
