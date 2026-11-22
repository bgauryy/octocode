# Developing Octocode

Owns the native crate map, contract pipeline, build commands, development-only environment variables and ownership rules. Repo-wide rules: `<repo>/AGENTS.md`; tool concept: [tool workflows](../../../README.md#how-an-agent-uses-octocode).

Other dev docs: [ADDING_CONFIG.md](ADDING_CONFIG.md) (new settings), [TOOL_QUALITY.md](TOOL_QUALITY.md) (acceptance criteria), [scripts/README.md](../scripts/README.md) (root automation), and the [benchmark](../../../packages/octocode-benchmark/README.md) (behavioral docs; stored results stay local, indexed in `packages/octocode-benchmark/results/README.md`).

## Runtime flow

CLI launcher and MCP launcher (stdio) both call the native Rust `ToolRuntime` (validate, execute, secure, shape), which serves the contract embedded at build time. The VS Code extension only sets up editor MCP and syncs GitHub sign-in.

`octocode-native` owns public tool validation, providers, security, batching, pagination, response shaping and cancellation. Interfaces only register, delegate, render or offer interactive selection.

## Native crates

Package map: `<repo>/AGENTS.md`. Cargo crates under `packages/octocode-native/crates/`:

| Crate dir | Owns |
|---|---|
| `runtime` (`octocode-native`) | Facade: tool catalog, admission, dispatch, build identity (`build.rs`), tests that cross a tool seam |
| `runtime-base` | Layers below the tools: validation, config resolution, GitHub auth, providers, security, response shaping, pagination; `build.rs` embeds `packages/octocode-config/contract/`; tool test fixtures (`test_support`, feature `test-support`) |
| `tools-shared` | Modules two or more tools use, layered by `SHARED_LAYERS` in `crates/runtime/src/tools/module_boundaries/tool_edges.rs` |
| `tool-*` | One crate per tool family (`tool-graph`, `tool-lsp`, `tool-github`, `tool-local`, `tool-ast`, `tool-packages`, `tool-clasify`); a tool imports no other tool |
| `test-support` | Fixtures no release build links (`mock_http`, engine conformance grammar); dev-dependency only |
| `engine` | Primitives: text search, tree-sitter AST, LSP client pool, minification, secret scanning, code graph |
| `io` | Low-level I/O, HTTP, resource primitives; no tool or provider policy |
| `contract-types`, `contract-base` | Generated tool wire types; no hand-written schema or logic |
| `github` | GitHub REST/GraphQL transport, bounded provider operations, credential traits |
| `cli` | The native `octocode` binary: tool commands, `schema`, `config`, `auth`, `graph`, `skill`, `install`, and the stdio MCP server (`mcp`, rmcp) |

## Concept to code

Under `packages/octocode-native/`: contracts are authored in core `src/toolContract/` and generated into `packages/octocode-config/contract/`; MCP server `crates/cli/src/cli/mcp_server*`; CLI dispatch `crates/cli/src/cli.rs`; validation `crates/runtime-base/src/contracts*`; gate and dispatch `crates/runtime/src/runtime/`; response shaping, continuations and paging `crates/runtime-base/src/response/`; path sandbox and redaction `crates/runtime-base/src/policy/`, `security/`, `crates/engine/src/security/`; internal limits `crates/runtime-base/src/execution/limits.rs`; loopback providers and the answer recorder `skills-dev/octocode-dev/scripts/providers/`.

## Contract pipeline

1. Author in core (Zod schema, description, instructions, limits) and build core.
2. `node skills-dev/octocode-dev/scripts/dev.mjs contracts:regen` at the repo root refreshes the configured core (the sibling package during local development or the exact npm pin after reset), then runs `@octocodeai/config generate:tool-contract`, the only generator. Output is `packages/octocode-config/contract/`: committed, never hand-edited; `check:tool-contract` fails when stale.
3. Rebuild native (`yarn workspace @octocodeai/octocode-native build:dev`): `crates/runtime-base/build.rs` embeds `contract/`. Before replacing a runnable host executable, staging checks its public `--version` against the package version and the first eight characters of the generated contract fingerprint. A mismatch leaves the existing executable in place.

Regeneration needs `cargo install cargo-typify --version 0.8.0 --locked`. `tool_types.rs` holds only the generated types native source names and what they reach; after a native edit that starts or stops naming one, regenerate. The binary embeds the contract it serves (CLI and `octocode mcp`), so there is no runtime override.

### Adding a tool field

1. Add it in core and regenerate as above.
2. Implement it, then declare its effect in `crates/runtime-base/src/contracts/field-effect-coverage.json`, or confirm it inherits its nearest declared ancestor's effect (a path with no entry inherits). `$DEV test:dev -- field_effect_coverage::` fails on an unresolved path or a stale key.
3. A public limit lives in core and reaches native as the contract's `limits`: change it there and regenerate (`check:tool-contract` fails on a stale copy). Tests assert the invariant the limit protects, never its value.
4. Check the result against [TOOL_QUALITY.md](TOOL_QUALITY.md), then update `<repo>/docs/OCTOCODE_TOOLS.md`.

A new configuration setting follows [ADDING_CONFIG.md](ADDING_CONFIG.md).

## Build, test, lint

From the repo root. Repo-wide tasks run through `node skills-dev/octocode-dev/scripts/dev.mjs <task>` (`$DEV`); task table: [SKILL.md](../SKILL.md#task-runner).

- `yarn workspace @octocodeai/octocode-native build:all`: release native binaries for all platforms (clears old binaries first). `$DEV build:publish` runs it.
- `yarn workspace @octocodeai/octocode-native check:target <platform>`: type-check one other platform (cargo-zigbuild or cargo-xwin) without linking; the fast loop for `cfg(windows)`/`cfg(target_os)` code. Details: [PUBLISHING.md](../../../packages/octocode-native/docs/PUBLISHING.md#build).
- `node skills-dev/octocode-dev/scripts/dev.mjs contracts:regen`: regenerate the tool contract from core.

| Package | Test | Lint / format |
|---|---|---|
| native | `test` (`test:scripts` + `test:node` + `test:rust`), `test:rust` alone (libtest, and nextest for the child-process tests), `coverage:rust`, `check:features`, `mutants` | `lint` (`lint:oxlint`, then `lint:own`: clippy `-D warnings`), `fmt:rust`, `fmt:rust:check`, `deny`, `shear` |
| config | `test`, `test:coverage`, `check:tool-contract`, `check:config-contract` | `lint` (`lint:own`: config contract check; then `lint:oxlint`; `build` checks the tool contract) |
| mcp, octocode, vscode | `test`, `test:coverage` (the floors: `verify:own` and `test:ci` run it) | `lint` (`lint:own`: prettier `--check` and the package's checks; then `lint:oxlint`), `format` (prettier `--write`) |
| chrome-devtools, claude-plugin, codex-plugin | `test` | `lint` (`lint:own`: prettier `--check` and the package's checks; then `lint:oxlint`), `format` (prettier `--write`) |
| agents-communication | `test` | `lint` (`lint:own`: catalog and generated-types checks; then `lint:oxlint`) |
| mcp-cli | `test`, `test:coverage` | `lint` (`lint:oxlint` only; no `lint:own`) |

`$DEV format` runs every workspace `format` script. Native's Rust types are checked by its clippy `lint`.

### Lint and typecheck rules

- **oxlint**: the root `.oxlintrc.json` is the only lint config; it turns on type-aware rules and denies warnings (`no-explicit-any`, `consistent-type-imports`, `no-floating-promises`, `import/no-cycle`, `no-console` except tests, `bin/`, `scripts/`, `tools/`, build files; `*.generated.*` ignored). Each workspace has `lint:oxlint` (`yarn run -T oxlint -c ../../.oxlintrc.json .`), `lint:own` (every other check: prettier, manifest and generator `--check`, syntax, clippy; absent in mcp-cli) and `lint` (both). `verify:own` is `verify` without oxlint (workspace-health requires both scripts). Repo-wide tasks run oxlint once, in the baseline pass, then each workspace's `lint:own` or `verify:own`.
- **oxlint baseline**: violations that predate a rule are recorded in `.oxlintrc.baseline.json` (the root config `extends` it), one override per file. The baseline only shrinks. `node skills-dev/octocode-dev/scripts/oxlint-baseline.mjs` fails on a stale entry and on a violation without one; `--write` removes stale entries but never adds one. Fix new violations in the code. The sibling core checkout (`../octocode-mcp-host/packages/octocode-core`) lints with copies of both files; this repo's are canonical, and `$DEV verify` runs `core-lint-sync.mjs`, which fails when core's `scripts/oxlint-baseline.mjs` is not byte-identical or its `.oxlintrc.json` rules differ (`$schema`, `ignorePatterns` and `packages/...`-scoped overrides are layout-specific), and skips without the sibling checkout.
- **TypeScript**: every project extends the root `tsconfig.base.json` (`strict`, `verbatimModuleSyntax`, `isolatedModules`); typecheck is `tsgo --noEmit`, and publishing packages still emit declarations with `tsc`.

CI runs `lint:ci` and `typecheck:ci`.

### Local static checks

| Command | What runs | When |
|---|---|---|
| `$DEV check` | The oxlint baseline, rustfmt, `lint:rust`, every other workspace's `lint:own`, and every typecheck, with workspaces in parallel. No build, no tests | Before handing over a change, with a focused test (`$DEV test:dev -- <filter>`) |
| `$DEV lint:rust` | Plain clippy, then the ratchet below | After a Rust edit, when `check` is more than you need |
| `$DEV verify` | Builds, every test, `deny`, `shear`, docs | Cross-package handoff |

**Clippy ratchet** (`scripts/rust-lint.mjs`, local only; CI and `cargo clippy` keep `-D warnings`). It adds production-only lints and bans (`packages/octocode-native/clippy-ratchet.toml`: fixed durations and sleeps, `Vec::truncate`, child processes, HTTP client construction, unbounded `fs::read`, non-atomic `fs::write`) through a config generated under `target/rust-lint/`. Findings that predate a rule are in `clippy-baseline.json`, counted per crate: a crate that grows fails and names the files; one that shrinks is stale until `--write`. A site that must stay says why: `#[expect(clippy::disallowed_methods, reason = "...")]`. Tests are not judged by the ratchet.

Not gated: clippy `pedantic`/`nursery` wholesale (pick lints into the ratchet instead), `indexing_slicing`, `arithmetic_side_effects`, `as_conversions`, `semgrep`. For one-off audits use `astSearch`; run `uvx ruff check --select F,B` and `uvx --from shellcheck-py shellcheck` by hand on scripts.

**Rules that stay tests:** `module_boundaries` resolves `use` paths and layers, and `no_error_code_is_spelled_as_a_string` reads JSON strings: clippy sees neither. Bans on a named call are the lint's job.

Sanity and end-to-end checks of the built CLI and MCP server are behavioral: an agent makes the calls and judges each answer by reading it, through the checklists in [tool checks](../../../packages/octocode-benchmark/CHECKS.md) (sanity after a build, end to end before a release). No harness script runs them. Package tests stay the regression net: native `cargo` tests with mocked providers and the packages' unit tests.

The native mocks replay real provider answers from `test-utils/provider-answers`. `$DEV answers:record [id,...]` re-records them live through the built CLI ([recorder](../scripts/providers/record.mjs), cases in `recorded-plan.mjs`); it refuses a recording that holds the token or a private repository, loses a field path when trimmed, or makes the tool ask anything else.

### Cargo builds

- `build:dev` (debug native + TypeScript, no clean or lint) is the default local build; use `build` only for release or performance artifacts. Check exit codes, not output paths.
- Judge speed and memory only on native `build` or `--profile profiling` (`build:dev` is opt-level 0).
- Rust integration tests are one binary per crate: `tests/main.rs` declares each `tests/*.rs` as a `mod`.
- Don't commit local lld or sccache Cargo configuration.
- Use the repository's shared `target/` and one build owner, per the [working rules](../../../AGENTS.md#working-rules). Other lanes run focused checks and reuse matching artifacts; do not create private targets or clean a concurrent build.
- At most two Rust builds run at once: `packages/octocode-native/scripts/build-slot.cjs` holds two slots in `os.tmpdir()/octocode-build-slots/`, shared by every session and isolated copy. `build:dev`, `build`, `test:rust`, `coverage:rust`, `$DEV test:dev`, `lint:rust` (and `check`), `lint:own`, `check:features`, `mutants`, `fuzz`, `miri` and `llvm-lines` hold one for their whole run; a waiting job prints the holders, a dead holder's slot is reclaimed, a nested wrapper reuses its slot. `OCTOCODE_BUILD_SLOTS` overrides the count. A bare `cargo build`, `test`, `clippy` or `nextest` takes no slot: run any other cargo build as `node scripts/build-slot.cjs -- cargo ...` in `packages/octocode-native`.
- `test:rust`, `coverage:rust` and each `$DEV test:dev` binary run with a private `TMPDIR` (`scripts/test-tmpdir.cjs`) removed at the end: a backstop only. A test removes what it creates (an RAII `tempfile` dir or the crate's `test_support` temp dir, never a hand-built path under `temp_dir()`), and a test that starts a warm server owns its socket namespace and stops it (`persistent_cli`).
- `[profile.dev] incremental = false` stops `.rcgu.o` accumulation ([rust-lang/rust#161824](https://github.com/rust-lang/rust/issues/161824)); never set `CARGO_INCREMENTAL=1`.
- `build:dev`, `test:rust` and `lint:rust` record the units they used in `target/.keep/`. At the end of a round the build owner runs `$DEV prune:rust` (`--dry-run` previews): it refuses while any cargo, rustc or nextest process runs, then deletes `deps/`, `.fingerprint/` and `build/` entries no keep record of the last 7 days lists, `incremental/` dirs, `target/rust-lint/{debug,tmp}` and files copied into `target/<profile>/`; it leaves everything outside `target/`, such as the staged `npm/<platform>/octocode`. `$DEV clean:cache` (our crates; dependencies stay warm) and `$DEV clean` (all build outputs) are the full resets, after coordinating with other sessions.
- Benchmark campaigns keep a `build/` snapshot so a resumed run uses the same build; remove it only when the campaign is final.
- After a `rust-toolchain.toml` bump, `cargo sweep --toolchains <new-toolchain> .` removes the old toolchain's builds, `target/platforms/` included.

### Rust dev tools

Every tool's version, and the dated nightly the opt-in tools use, is pinned in `packages/octocode-native/dev-tools.json`. `$DEV rust:tools` compares it with what is installed and prints the `cargo install --locked <tool>@<version>` (or `rustup`) command for anything missing or different; nothing installs automatically. CI installs the tools it gates on at the same pins. Prebuilt binaries are fine at the pinned version. Scripts live in `packages/octocode-native`; run them with `yarn workspace @octocodeai/octocode-native <script>`.

Run in `packages/octocode-native` via `yarn workspace @octocodeai/octocode-native <script>` unless `$DEV`:

- cargo-nextest: `test:rust` (tests that start child processes, `PROCESS_TESTS` in `scripts/rust-tests.cjs`; `.config/nextest.toml`, CI `NEXTEST_PROFILE=ci`). Every change; CI.
- cargo-shear `shear` (unused dependencies, unlinked files, doctests under `doctest = false`) and cargo-deny `deny` (`deny.toml`): `verify`; CI.
- cargo-llvm-cov: `coverage:rust [--html | --lcov --output-path lcov.info | --summary-only]` (`target/llvm-cov-target` is large; delete when done).
- cargo-hack: `check:features`, after changing `[features]` or `#[cfg(feature)]`.
- cargo-mutants: `$DEV mutants --in-diff <diff>` or `-f <file>` (`.cargo/mutants.toml`); manual, copies and builds the tree, never per commit.
- samply: `$DEV profile` (`[profile.profiling]`). bacon: `bacon`, `bacon clippy`, `bacon test` (`bacon.toml`).
- cargo-fuzz (nightly): `$DEV fuzz:smoke` (60 s/target), `$DEV fuzz` (600 s); `[--seconds N] [--target-dir D] [target...]`; `fuzz/`, bounds in `dev-tools.json`. After changing code a target covers; never CI.
- Miri (nightly): `$DEV miri` (deadline 1500 s; `miri` in `dev-tools.json`), after changing `unsafe`, offset or concurrency code it covers.
- cargo-llvm-lines: `$DEV llvm-lines [package] [--timings]` (`target/llvm-lines`, delete when done); compile time or binary size, not a gate.

`scripts/rust-tests.cjs` holds the package and feature selections for `test:rust` and `coverage:rust`; a script test keeps the `bacon.toml` test jobs and the `cargo dev-test` alias equal to them. `test:rust` builds once (`cargo test --no-run`), then runs libtest over every test but `PROCESS_TESTS` (binaries side by side, each under the `scripts/test-binaries.cjs` watchdog) and nextest over `PROCESS_TESTS` at the same time. A test that shares a process keeps no process-global state: a hook is thread-local or a guard that restores it on drop, a test forgets only what its own calls kept (its own temp workspace is its own store key), and time it asserts on is a frozen or injected clock or a count of work, never the wall clock. `$DEV test:dev [--crate <name>]... [--lib] -- <filter>` runs the same build, then only the named crates' test binaries (all without `--crate`; unit-test binaries only with `--lib`) in a build slot; `scripts/test-watchdog.cjs` fails a test still unfinished 10 minutes after libtest's slow-test line. `cargo dev-test` and `bacon` run cargo directly, outside the slots. Library crates set `doctest = false` (nextest runs no doctests; `shear` fails if one appears).

Fuzz, Miri: `packages/octocode-native/fuzz` is its own workspace (the main one `exclude`s it); only `fuzz/seeds/<target>` is committed (`fuzz/corpus/`, `fuzz/artifacts/` are gitignored). A crash becomes a deterministic test in the crate it exercises (for example `text/utf8_offsets/tests/fuzz_regressions.rs`), never a committed input. Miri runs only the selections in `dev-tools.json` (what it cannot run is listed there under `unsupported`); shorten a test's loop under `cfg!(miri)` instead of skipping it.

## Profiling a tool flow

One command, macOS and Linux:

```bash
node scripts/build-slot.cjs -- cargo build --locked --profile profiling -p octocode-cli   # in packages/octocode-native; --build skips the slot
$DEV profile localSearch '{"path":"crates","matchString":["fn "]}' --cwd packages/octocode-native --phase repeat
samply load .octocode/octocode-dev/profile/<run>/profile.json.gz   # the flamegraph
```

`--phase` picks what is measured: `first` (a fresh MCP server's first call), `repeat`, `page:N`, or `one` (a whole one-shot CLI process). It prints min-of-3 retired instructions and peak footprint measured without a sampler, then samples a server looping that phase and writes `top.md` (self and inclusive CPU shares, loop only) beside `counters.json` under `.octocode/octocode-dev/profile/`. Wall time is noise on a shared machine: trust instructions.

For a graph cursor profile, put the snapshot in the target workspace’s graph store, use that workspace as `--cwd`, and request `--phase page:2` on a query that issues a continuation. Check the counters’ error list before comparing instruction counts.

A plain CLI tool call is a thin client of the shared warm `octocode serve` process (`crates/cli/src/cli/serve.rs`), so separate CLI processes are not separate runs and `time -l` measures nothing useful. Measure through `$DEV profile` or a fresh `octocode mcp` process. `OCTOCODE_STORAGE_MODE=memory` runs in-process but refuses any answer that needs a continuation (`executionFailed`), so it cannot measure large answers.

`--heap` adds what the server still holds after the flow (`malloc_history` on macOS, `heaptrack` on Linux): retained memory, not a transient peak (peak is in the counters).

## Development environment variables

These are not user settings and are not read from `.octocoderc`. User settings are in `<repo>/docs/CONFIGURATION.md`.

| Variable | Read by | Effect |
|---|---|---|
| `OCTOCODE_NATIVE_BIN` | `octocode` and `octocode-mcp` launchers | Absolute path to a native `octocode` binary used instead of the platform package. Ignored when `NODE_ENV=production`; the bundled launchers honor it only with `NODE_ENV` `development` or `test`. An installed `octocode` whose postinstall made its bin the native binary never reads it: run `node <package>/out/octocode.js` |
| `XDG_CONFIG_HOME` | native `octocode install` (Linux) | Locates IDE config directories (default `~/.config`); never moves the Octocode home |

## Ownership rules

- `crates/engine` exposes primitives, not tool policy.
- `crates/cli/src/cli/skill/` owns skill commands and filesystem behavior. Its build script embeds public `skills/` through the same staging filter used by the Node package; Node is needed at build time, not for native skill commands.
- Each doc topic has one owner; see `<repo>/docs/README.md`. Per-package invariants are in each package `ARCHITECTURE.md`.
