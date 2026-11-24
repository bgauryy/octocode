# Developing Octocode

Where code lives, the contract pipeline, builds, checks and profiling. Repo rules and the package map: `<repo>/AGENTS.md`. `$DEV` and `$OCTO` are defined there; run from the repo root.

## Where code lives

`crates/` is `packages/octocode-native/crates/`; `CORE` is `../octocode-mcp-host/packages/octocode-core`. A tool crate imports no other tool; code two tools share goes in `tools-shared`, layered by `SHARED_LAYERS` (`crates/runtime/src/tools/module_boundaries/tool_edges.rs`). `engine` holds primitives (text search, tree-sitter, LSP client pool, minification, secret scanning, code graph), never tool policy; `io` holds I/O and HTTP primitives; `test-support` holds fixtures no release build links (a dev-dependency only).

| Piece | Path |
|---|---|
| Tool definition, description, field `.describe()` text | `CORE/src/toolContract/tools/<tool>.ts`; shared builders `tools/_*.ts`; published forms `published{,Forms,Json}.ts` |
| Native rule catalog | `CORE/src/toolContract/nativeRules/` |
| Instructions | `CORE/src/toolContract/instructions.ts` (`MCP_INSTRUCTIONS`, plus `MCP_TOOL_INSTRUCTIONS` per optional tool); the CLI appends its `<cli>` line in `crates/cli/src/cli/schema.rs` |
| Output schemas, public limits | `CORE/src/toolContract/{outputSchemas,outputEnvelope,outputResponse,limits}.ts` |
| Generated contract and wire types | `packages/octocode-config/contract/`, embedded by `crates/runtime-base/build.rs`; types in `contract-types`, `contract-base` |
| Field-effect index | `crates/runtime-base/src/contracts/field-effect-coverage.json`, gated by `crates/runtime/tests/field_effect_coverage.rs` |
| Validation | `crates/runtime-base/src/contracts/` (`prepare.rs`, `validate.rs`, `normalize.rs`) |
| Catalog, admission, dispatch, cursors, page replay | `crates/runtime/src/runtime/` (`dispatch.rs`, `domain_dispatch.rs`, `engine/`) |
| Tool code | `crates/tool-<family>/src/<snake_tool>/` (`local`, `ast`, `lsp`, `github`, `packages`, `graph`, `clasify`); clasify judgment in `crates/tools-shared/src/{judgment,source_capture}/` |
| GitHub transport, quota budget, answer reuse | `crates/github/` (`transport/saved.rs` decides when a saved answer may answer again) |
| Providers, saved-answer store | `crates/runtime-base/src/providers/` |
| Response shaping, pages, continuations, partial marks | `crates/runtime-base/src/response/` (`paging.rs`, `continuations.rs`, `cursor.rs`) |
| Path sandbox, redaction | `crates/runtime-base/src/{policy,security}/`, `crates/engine/src/security/` |
| Internal limits | `crates/runtime-base/src/execution/limits.rs` (`RuntimeLimits::DEFAULT`) |
| CLI and stdio MCP server | `crates/cli/src/cli.rs`, `crates/cli/src/cli/mcp_server*`; launchers `packages/octocode/src/cli/native-delegate.ts`, `packages/octocode-mcp/src/index.ts` |
| Skill commands | `crates/cli/src/cli/skill/`; its build script embeds public `skills/` through the Node staging filter (Node needed at build time) |
| Settings | [ADDING_CONFIG.md](ADDING_CONFIG.md) |
| Loopback providers, answer recorder | `skills-dev/octocode-dev/scripts/providers/` |
| Tests | core `CORE/src/__tests__/` (`publishedSurfaceBudget` holds the context cost); native `crates/runtime/tests/`, `crates/cli/tests/` (each crate's integration tests are one binary: `tests/main.rs` declares every file as a `mod`) |

## Contract pipeline

1. Author in core (Zod schema, description, instructions, limits) and build core.
2. `$DEV contracts:regen` refreshes the configured core (the sibling during local development, the npm pin after `$DEV setup --reset`) and runs `@octocodeai/config generate:tool-contract`, the only generator. `check:tool-contract` fails on a stale `contract/`.
3. `$DEV build:dev`. Before replacing a runnable host executable, staging compares its `--version` with the package version and the first eight characters of the contract fingerprint; on a mismatch the old executable stays.

`tool_types.rs` holds only the generated types native source names and what they reach: after a native edit that starts or stops naming one, regenerate. The binary embeds the contract it serves; there is no runtime override.

A new tool field: add it in core and regenerate, implement it, then declare its effect or let it inherit its nearest declared ancestor's (`$DEV test:dev -- field_effect_coverage::` fails on an unresolved path or a stale key). A public limit lives in core `limits` and reaches native through the contract. Update `<repo>/docs/OCTOCODE_TOOLS.md`.

## Checks

- **oxlint**: the root `.oxlintrc.json` is the only config (type-aware, warnings denied). Older violations sit in `.oxlintrc.baseline.json`, one override per file; it only shrinks: `scripts/oxlint-baseline.mjs` fails on a stale entry or an unlisted violation, and `--write` only removes entries. The sibling core lints with copies of both files; `$DEV verify` runs `core-lint-sync.mjs`, which fails on drift and skips without the sibling.
- **Workspace scripts**: `lint:oxlint`, `lint:own` (every other check; none in mcp-cli), `lint` (both), `verify:own` (`verify` without oxlint), `test:coverage` (floors; `test:ci` runs it). Typecheck is `tsgo --noEmit` over the root `tsconfig.base.json`. CI runs `lint:ci` and `typecheck:ci`.
- **Clippy ratchet** (`$DEV lint:rust`, `scripts/rust-lint.mjs`, local only; CI keeps `-D warnings`): production-only lints and bans from `packages/octocode-native/clippy-ratchet.toml` (fixed sleeps, `Vec::truncate`, child processes, HTTP client construction, unbounded `fs::read`, non-atomic `fs::write`). Older findings are counted per crate in `clippy-baseline.json`: growth fails and names the files, a shrink is stale until `--write`, `--adopt` adds a newly listed lint. A site that must stay says why: `#[expect(clippy::disallowed_methods, reason = "...")]`. Tests are not judged. Clippy `pedantic` and `nursery` are never enabled wholesale: adopt single lints into the ratchet.
- **Rules that stay tests**: `module_boundaries` (use paths and layers) and `no_error_code_is_spelled_as_a_string` (JSON strings); clippy sees neither.
- **Recorded provider answers**: native mocks replay `test-utils/provider-answers`. `$DEV answers:record [id,...]` re-records them live through the built CLI (cases in `scripts/providers/recorded-plan.mjs`); it refuses a recording that holds the token or a private repository, loses a field path when trimmed, or makes the tool ask anything else.

## Cargo builds

- Judge speed and memory only on `build` or `--profile profiling`; `build:dev` is opt-level 0.
- Machine-specific linker or sccache settings: copy `packages/octocode-native/.cargo/config.local.example.toml` to `config.local.toml` (gitignored).
- Build slots live in `os.tmpdir()/octocode-build-slots/`, shared by every session and isolated copy. `build:dev`, `build`, `test:rust`, `coverage:rust`, `test:dev`, `lint:rust` (and `check`), `lint:own`, `check:features`, `mutants`, `fuzz`, `miri` and `llvm-lines` each hold one for the whole run; a dead holder's slot is reclaimed, a nested wrapper reuses its slot, `OCTOCODE_BUILD_SLOTS` overrides the count.
- JS/TS test scripts run through `scripts/test-tmp.mjs`: `TMPDIR` (and Chromium's `MAC_CHROMIUM_TMPDIR`) point at `.octocode/tmp/t/<package>-<pid>/`, removed when the run ends; a checkout path too long for a unix socket under it is refused. `agents-communication` and `test:rust` keep their own roots (see `TODO.md`).
- `test:rust`, `coverage:rust` and `test:dev` give each binary a private `TMPDIR` (`scripts/test-tmpdir.cjs`) removed at the end, as a backstop only; a test that starts a warm server owns its socket namespace and stops it (`persistent_cli`).
- Wrappers record the units they used in `target/.keep/` and print `deps: N entries`; a climbing count means stray selections. `$DEV prune:rust` deletes `deps/`, `.fingerprint/` and `build/` entries no recent keep record lists, plus `incremental/`, `target/rust-lint/{debug,tmp}` and copied binaries; it leaves the staged `npm/<platform>/octocode`. `$DEV clean:cache` and `$DEV clean` are full resets: coordinate first.
- Benchmark campaigns keep a `build/` snapshot so a resumed run uses the same build; remove it when the campaign is final.
- After a `rust-toolchain.toml` bump, `cargo sweep --toolchains <new-toolchain> .` removes the old toolchain's builds, `target/platforms/` included.
- Another platform: `yarn workspace @octocodeai/octocode-native check:target <platform>` type-checks without linking ([PUBLISHING.md](../../../packages/octocode-native/docs/PUBLISHING.md#build)); `build:all` (via `$DEV build:publish`) is release work.

### Rust dev tools

Versions and the dated nightly are pinned in `packages/octocode-native/dev-tools.json`; `$DEV rust:tools` prints the install command for anything missing or different and installs nothing. Package scripts run as `yarn workspace @octocodeai/octocode-native <script>`.

- `test:rust`: one `cargo test --no-run` build, then libtest over every test binary (each under the `scripts/test-binaries.cjs` watchdog) and nextest over `PROCESS_TESTS` (`scripts/rust-tests.cjs`, which also holds the selections) at once. `scripts/test-watchdog.cjs` fails a test still running 10 minutes after libtest's slow-test line. A script test keeps `bacon.toml` and the `cargo dev-test` alias equal to the selections.
- `shear` (unused dependencies, unlinked files; library crates set `doctest = false`, and a doctest fails it) and `deny` (`deny.toml`) run in `verify` and CI.
- `coverage:rust [--html | --lcov --output-path lcov.info | --summary-only]`; delete `target/llvm-cov-target` afterwards.
- `check:features` (cargo-hack) after changing `[features]` or `#[cfg(feature)]`.
- `$DEV mutants --in-diff <diff>` or `-f <file>` (`.cargo/mutants.toml`): manual, copies and builds the tree.
- `$DEV fuzz:smoke` (60 s per target), `$DEV fuzz` (600 s); `fuzz/` is its own workspace and commits only `fuzz/seeds/<target>`. A crash becomes a deterministic test in the crate it exercises, never a committed input.
- `$DEV miri` runs the selections in `dev-tools.json` (what it cannot run is listed under `unsupported`); shorten a loop under `cfg!(miri)` instead of skipping the test.
- `$DEV llvm-lines [package] [--timings]` for compile time or binary size; delete `target/llvm-lines` afterwards.

## Profiling a tool flow

```bash
node scripts/build-slot.cjs -- cargo build --locked --profile profiling -p octocode-cli   # in packages/octocode-native
$DEV profile localSearch '{"path":"crates","matchString":["fn "]}' --cwd packages/octocode-native --phase repeat
samply load .octocode/octocode-dev/profile/<run>/profile.json.gz
```

`--phase` is `first` (a fresh MCP server's first call), `repeat`, `page:N` or `one` (a whole one-shot CLI process). The run prints min-of-3 retired instructions and peak footprint without a sampler, then samples a server looping that phase and writes `top.md` and `counters.json` under `.octocode/octocode-dev/profile/`. Trust instructions, not wall time. `--heap` adds what the server still holds afterwards (`malloc_history` on macOS, `heaptrack` on Linux).

A plain CLI call is a thin client of the shared warm `octocode serve` process (`crates/cli/src/cli/serve.rs`), so `time -l` on it measures nothing useful. `OCTOCODE_STORAGE_MODE=memory` runs in-process but refuses any answer that needs a continuation, so it cannot measure large answers. For a graph cursor, put the snapshot in the target workspace's graph store, use that workspace as `--cwd`, request `--phase page:2`, and check the counters' error list before comparing.

## Development environment variables

Not user settings and never read from `.octocoderc` (user settings: `<repo>/docs/CONFIGURATION.md`).

| Variable | Read by | Effect |
|---|---|---|
| `OCTOCODE_NATIVE_BIN` | `octocode` and `octocode-mcp` launchers | Absolute path to a native binary used instead of the platform package, honored only with `NODE_ENV` `development` or `test`. An installed `octocode` whose postinstall made its bin the native binary never reads it: run `node <package>/out/octocode.js` |
| `XDG_CONFIG_HOME` | native `octocode install` (Linux) | Locates IDE config directories (default `~/.config`); never moves the Octocode home |
