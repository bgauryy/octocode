Load when building, testing or pruning Rust, running a Rust dev tool, or pointing a launcher at a locally built binary.

# Rust builds and dev tools

## Cargo builds

- Judge speed and memory only on `build` or `--profile profiling`; `build:dev` is opt-level 0.
- Machine-specific linker or sccache settings: copy `packages/octocode-native/.cargo/config.local.example.toml` to `config.local.toml` (gitignored).
- Build slots live in `os.tmpdir()/octocode-build-slots/`, shared by every session and isolated copy. `build:dev`, `build`, `test:rust`, `coverage:rust`, `test:dev`, `lint:rust` (and `check`), `lint:own`, `check:features`, `mutants`, `fuzz`, `miri` and `llvm-lines` each hold one for the whole run; a dead holder's slot is reclaimed, a nested wrapper reuses its slot, `OCTOCODE_BUILD_SLOTS` overrides the count.
- JS/TS test scripts run through `<repo>/scripts/test-tmp.mjs`: `TMPDIR` (and Chromium's `MAC_CHROMIUM_TMPDIR`) point at `.octocode/tmp/t/<package>-<pid>/`, removed when the run ends; a checkout path too long for a unix socket under it is refused. `agents-communication` and `test:rust` keep their own roots (see `TODO.md`).
- `test:rust`, `coverage:rust` and `test:dev` give each binary a private `TMPDIR` (`packages/octocode-native/scripts/test-tmpdir.cjs`) removed at the end, as a backstop only; a test that starts a warm server owns its socket namespace and stops it (`persistent_cli`).
- Wrappers record the units they used in `target/.keep/` and print `deps: N entries`; a climbing count means stray selections. `$DEV prune:rust` deletes `deps/`, `.fingerprint/` and `build/` entries no recent keep record lists, plus `incremental/`, `target/rust-lint/{debug,tmp}` and copied binaries; it leaves the staged `npm/<platform>/octocode`. `$DEV clean:cache` and `$DEV clean` are full resets: coordinate first.
- Benchmark campaigns keep a `build/` snapshot so a resumed run uses the same build; remove it when the campaign is final.
- After a `rust-toolchain.toml` bump, `cargo sweep --toolchains <new-toolchain> .` removes the old toolchain's builds, `target/platforms/` included.
- Another platform: `yarn workspace @octocodeai/octocode-native check:target <platform>` type-checks without linking ([PUBLISHING.md](../../../packages/octocode-native/docs/PUBLISHING.md#build)); `build:all` (via `$DEV build:publish`) is release work.

## Rust dev tools

Versions and the dated nightly are pinned in `packages/octocode-native/dev-tools.json`; `$DEV rust:tools` prints the install command for anything missing or different and installs nothing. Package scripts run as `yarn workspace @octocodeai/octocode-native <script>`; `scripts/` in this section is `packages/octocode-native/scripts/`.

- `test:rust`: one `cargo test --no-run` build, then libtest over every test binary (each under the `scripts/test-binaries.cjs` watchdog) and nextest over `PROCESS_TESTS` (`scripts/rust-tests.cjs`, which also holds the selections) at once. `scripts/test-watchdog.cjs` fails a test still running 10 minutes after libtest's slow-test line. A script test keeps `bacon.toml` and the `cargo dev-test` alias equal to the selections.
- `shear` (unused dependencies, unlinked files; library crates set `doctest = false`, and a doctest fails it) and `deny` (`deny.toml`) run in `verify` and CI.
- `coverage:rust [--html | --lcov --output-path lcov.info | --summary-only]`; delete `target/llvm-cov-target` afterwards.
- `check:features` (cargo-hack) after changing `[features]` or `#[cfg(feature)]`.
- `$DEV mutants --in-diff <diff>` or `-f <file>` (`.cargo/mutants.toml`): manual, copies and builds the tree.
- `$DEV fuzz:smoke` (60 s per target), `$DEV fuzz` (600 s); `fuzz/` is its own workspace and commits only `fuzz/seeds/<target>`. A crash becomes a deterministic test in the crate it exercises, never a committed input.
- `$DEV miri` runs the selections in `dev-tools.json` (what it cannot run is listed under `unsupported`); shorten a loop under `cfg!(miri)` instead of skipping the test.
- `$DEV llvm-lines [package] [--timings]` for compile time or binary size; delete `target/llvm-lines` afterwards.

## Development environment variables

Not user settings and never read from `.octocoderc` (user settings: `<repo>/docs/CONFIGURATION.md`).

| Variable | Read by | Effect |
|---|---|---|
| `OCTOCODE_NATIVE_BIN` | `octocode` and `octocode-mcp` launchers | Absolute path to a native binary used instead of the platform package, honored only with `NODE_ENV` `development` or `test`. An installed `octocode` whose postinstall made its bin the native binary never reads it: run `node <package>/out/octocode.js` |
| `XDG_CONFIG_HOME` | native `octocode install` (Linux) | Locates IDE config directories (default `~/.config`); never moves the Octocode home |
