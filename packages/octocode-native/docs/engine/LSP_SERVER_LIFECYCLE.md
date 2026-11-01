# LSP lifecycle and provisioning

How the engine finds, starts, pools, and bounds language servers. The public query contract and user-facing routing are in [`lspSearch`](../../../../docs/OCTOCODE_TOOLS.md#lspsearch); the LSP settings are in [CONFIGURATION.md](../../../../docs/CONFIGURATION.md); grammar coverage is in [Supported languages and features](SUPPORTED_LANGUAGES_AND_FEATURES.md).

## Semantic boundary

Tree-sitter and Oxc answer syntactic questions from embedded parsers. LSP operations launch a real language server over stdio to resolve cross-file identity, definitions, references, types, implementations, and call relationships. These evidence classes are not interchangeable.

When an operation requires an unavailable server, `lspSearch` returns an error row with `errorCode:"serverUnavailable"` and valid recovery calls. It never labels same-file or syntactic guesses as semantic results. A running server that lacks the requested capability returns `capabilityUnavailable`. A supported server returning no rows establishes only an empty result within that server's indexed scope and configuration.

`documentSymbols` also requires a server with the corresponding negotiated LSP capability. Use `localFetch view:"symbols"` for native syntax outlines and Markdown headings without a server. Markdown has no built-in LSP route; add a custom server entry to use it through `lspSearch`.

Public positions, input and output, are one-based, and servers communicate in UTF-16. A server selecting an unsupported encoding fails startup.

## Resolution

The Rust engine owns command discovery and launch configuration. For a known extension it resolves, in priority order:

1. an explicit `lsp.configPath` / `$OCTOCODE_LSP_CONFIG` entry;
2. `<workspace>/.octocode/lsp-servers.json` only when `lsp.trustProjectConfig` (`OCTOCODE_TRUST_PROJECT_LSP_CONFIG=true`) is set;
3. `<octocode home>/lsp-servers.json`;
4. the built-in route: a native `tsgo` for TypeScript, an installed pyright-family server for Python, a managed install, or the known command on `PATH` (for `typescript-language-server`, also a `node_modules` tree; see below).

Project configuration is untrusted by default because it controls executable startup; a trusted project configuration can launch any executable it names. Generic interpreter eval forms such as `node -e` or `python -c` and shell wrappers are rejected as a guard against mistakes, not as a boundary. Absolute commands must exist and be regular executable files; a relative command with a path separator resolves against the workspace root when the process directory has none.

The built-in routing table includes:

| Files | Command |
|---|---|
| TypeScript | octocode's bundled `tsgo --lsp -stdio` (a trusted workspace's own `node_modules/.bin/tsgo` first), else `typescript-language-server --stdio` |
| JavaScript | `typescript-language-server --stdio` |
| Python | `basedpyright-langserver --stdio`, else `pyright-langserver --stdio`, else `pylsp` |
| Rust | `rust-analyzer` |
| Go | `gopls serve` |
| Java | `jdtls` |
| C / C++ | `clangd` |
| C# | `csharp-ls` |
| Scala (`scala`, `sc`, `sbt`) | `metals` |

A `lsp-servers.json` entry for an extension replaces its built-in server, for example `{"languageServers":{".ts":{"command":"tsgo","args":["--lsp","-stdio"],"languageId":"typescript"}}}`.

Python picks the first installed server in that order. Pyright-family servers are looked up on `PATH` only, so a checkout cannot swap in a server it ships in `node_modules/.bin`; add an `lsp-servers.json` entry to use a workspace-local server. `pylsp` is the fallback. basedpyright and pyright implement call hierarchy, `workspace/symbol`, and implementation, and pylsp implements none of them. Custom configuration still wins over this order.

rust-analyzer starts headless: whenever the resolved server is rust-analyzer, `initializationOptions` default to `cargo.buildScripts.enable:false`, `procMacro.enable:false`, `checkOnSave:false`, and `cargo.targetDir:true`, so it runs no `build.rs`, proc-macro, or `cargo check` from the repository. Cache priming keeps rust-analyzer's default (on): the server indexes the workspace in the background as soon as it loads, so a prewarmed server answers the first workspace-wide request warm. User-supplied options merge on top and win key by key. `lspSearch` `rustContext` opts back in to build scripts and proc-macros.

Server startup clears the inherited environment and forwards the OS path/home variables plus `RUSTUP_HOME`, `CARGO_HOME`, and `RUSTUP_TOOLCHAIN`. Rust discovery and startup therefore use the same toolchain even with a custom home. Explicit server `env` entries override these values; credential variables are not inherited. Changing a forwarded toolchain setting gives the CLI a separate warm server.

clangd, jdtls, and Metals also start headless, on every launch path, with user arguments and options winning:

- **clangd** gets `--background-index=false` (no `.cache/clangd/` index written into the repository), `--clang-tidy=false`, `--log=error`, and `--pch-storage=memory`. A flag the user already passes, by name, is not added again.
  For CUDA files (`.cu`, `.cuh`) it also gets `initializationOptions.fallbackFlags` `["-xcuda","--cuda-host-only"]`. Without a compile command clangd invents `clang <file>`, which takes `.cuh` for a CUDA header and plans no compile job (`invalid AST`); host-only plans one job on any machine and, when a CUDA SDK is installed, clang still finds it and force-includes its headers. The flags apply only when no compile command applies (a `compile_commands.json` or `compile_flags.txt` entry overrides them), a user's own `fallbackFlags` replace them, and the pool key includes the language, so C and C++ files keep their own clangd without them. Without an SDK, clang reports the missing installation and `__global__` and `blockIdx` stay unresolved; a compile database is the fix.
- **jdtls** gets `-data <octocode home>/lsp-workspaces/jdtls/<sha256(workspace)[..16]>`, so its workspace data never lands in the repository, unless the user passes `-data`. Its `initializationOptions.settings` default to `java.autobuild.enabled:false` and `java.import.generatesMetadataFilesAtProjectRoot:false`.
- **Metals** gets `initializationOptions` `isHttpEnabled:false` and `statusBarProvider:"off"`. Metals may still create `.metals/` and `.bloop/` in the workspace; that is its documented project layout.

TypeScript and JavaScript routing is documented once, in [TypeScript backends](../../../../docs/OCTOCODE_TOOLS.md#typescript-backends). Engine specifics:

- The install tree that supplies the bundled `tsgo` (`@typescript/native-preview`, a dependency of `octocode-mcp` and `octocode`, pinned exactly because it is a nightly prerelease) and any `typescript-language-server` installed there (it is not a runtime dependency, so installs usually take it from `PATH`) is found from the running native executable (`lsp/module_path.rs`, `current_exe()`) for both CLI and MCP.
- A workspace's own `tsgo` is used only with `lsp.trustProjectConfig`. Its `typescript-language-server` is also used when the workspace is the directory Octocode started in, or inside it; a start directory that is `/` or the home directory trusts nothing under it. An untrusted checkout (a clone, say) never supplies either.
- When the chosen `tsgo` fails to start, the request falls back to `typescript-language-server` and the answer says so; an `lsp-servers.json` entry gets no substitute.
- tsgo answers independent requests in parallel, so importer verification sends up to 8 files at once to it; `typescript-language-server` gets one request at a time.
- `typescript-language-server` runs as `node <lib/cli.mjs>`, so the file needs no execute bit (package managers often install it mode 644). When it cannot start, the error names where the engine looked.

Octocode bundles no other language server. A known route does not mean the server is installed: install it in the workspace or toolchain, put it on `PATH`, add an `lsp-servers.json` entry, or use managed provisioning where supported.

`subtypes` and `implementation` on a type race the server against a syntax scan of the files that name it, with a 20 s grace for the server after the scan ends; behavior and labels are in [the tools reference](../../../../docs/OCTOCODE_TOOLS.md#subtypes-and-implementations-without-an-answer-in-time).

## Managed provisioning

The native CLI owns a pinned managed manifest for `rust-analyzer` and `clangd`:

```bash
npx octocode lsp-server list
npx octocode lsp-server status path/to/file.ts
npx octocode lsp-server install rust-analyzer clangd   # --all for every server
npx octocode lsp-server uninstall clangd
npx octocode lsp-server clean --yes                     # without --yes: a dry run
```

Installation is explicit: `lspSearch` never downloads an executable. `lsp.autoInstall` decides whether an install command may download without asking (`install --force` skips that policy for one run; an installed server is never re-downloaded: `uninstall` it first), and `lsp.cacheDir` moves the installs from `<octocode home>/lsp`.

Provisioning permits only allowed HTTPS hosts, follows the same restriction across redirects, requires a pinned SHA-256, supports bounded `none`, `gz`, and `zip` extraction, uses per-target locks, and atomically publishes a completion-marked executable. Unsupported platforms fail with an installation hint.

## Custom configuration

A configuration maps file extensions to launch specs:

```json
{
  "languageServers": {
    ".php": {
      "command": "intelephense",
      "args": ["--stdio"],
      "languageId": "php"
    }
  }
}
```

`command` and `languageId` are required. `args` defaults to an empty array and `initializationOptions` is passed to `initialize`. The example adds a route for a language with no built-in route; it does not add a grammar. A custom entry overrides built-in routing for that extension but does not bypass executable validation.

## Pool ownership

`packages/octocode-native/crates/engine/src/lsp/pool/` owns the complete pool lifecycle. One key identifies the server command, arguments, workspace, initialization options, and effective environment.

- Concurrent starts and health checks deduplicate.
- Active requests prevent idle shutdown.
- Successful use renews idle expiry and drives least-recently-used eviction.
- Clearing a key invalidates pending starts and waiters; late completions cannot publish stale clients.
- The sampled resident memory of all pooled servers together has a budget (`LspPoolOptions::max_resident_bytes`: half the host's memory, never under one server's default cap and never over `max_entries` such caps). While the total is over it, the pool stops idle least-recently-used servers; it never stops the server being acquired or a busy one, and overflows instead.
- Failed health checks evict the client so the next acquisition starts a replacement.
- clangd reads a file's compile command (`compile_commands.json`, `build/compile_commands.json`, `compile_flags.txt`, `.clangd`) once, when the document is opened, ignores `workspace/didChangeWatchedFiles` for them, and registers no watchers. The client therefore records those files in the directory of every document it syncs and in each ancestor; when one is created, changed, or deleted, the pool stops that server on the next acquisition **if it is idle** (a busy one keeps serving and is replaced on a later idle acquisition) and starts a fresh one. Other servers declare no such files and are never replaced this way.
- The readiness wait (the project load) runs on the installed client, after the start: a caller cancelled during it leaves a started, indexing server in the pool. A wait that ends in `timeout` keeps the client pooled; the next acquisition waits again, up to 15 s, so neither the stored `timeout` nor a language's whole budget costs every later call.
- A server that registers for `workspace/didChangeWatchedFiles` (`client/registerCapability`) gets file events without a watcher: before each request the client walks the scope (family and project files, ignored files included, bounded) and sends the created, changed and deleted files since the previous request of the same process; the first request records the snapshot only. rust-analyzer watches the workspace itself (`files.watcher: "server"`). Open documents are re-synced from disk before every request.
- Runtime shutdown stops every client and joins owned work.

Long-lived MCP sessions reuse warm servers. The CLI shares them through the [warm tool server](../../README.md#warm-tool-server) (`octocode serve`): one server per workspace and configuration, whose runtimes (one per working directory and environment) share one pool, so later calls reach servers already loaded.

## Resource containment

Language-server frames, writes, notifications, stderr, cancellation, and process teardown are bounded. Spawned servers run under a child-memory cap: a quarter of host RAM, between 4 and 8 GiB by default. A `maxMemoryMb` entry in `lsp-servers.json` overrides it per server; `0` disables it. No tool input reaches it. The cap bounds memory the server tree uses, not address space it only reserves. On macOS and Linux an RSS watchdog samples the resident memory of the server and all its descendants every 2 s; over the cap it fails the connection with a typed `ResourceExhausted` error (`language server exceeded memory cap (… MiB resident > … MiB)`) and SIGKILLs the whole tree. The pool then replaces the server on the next acquisition. `lspSearch` reports it as a final `serverCrashed` row (not retryable) that leads to an unverified text search of the symbol. Windows keeps a Job Object with `JOB_OBJECT_LIMIT_JOB_MEMORY` (committed memory) and kill-on-close behavior. Other Unix targets have no watchdog and apply `RLIMIT_AS` before `exec` as their only bound.

`RLIMIT_AS` is not used on macOS or Linux. It limits reserved virtual address space, and servers reserve far more than they touch: a JVM (jdtls) reserves its maximum heap (a quarter of host RAM by default) plus class space and code cache, V8 reserves WebAssembly memory cages, and Go and glibc reserve arenas. A 4 GiB address-space cap stops such servers from starting on large hosts while they use a fraction of it. On macOS it cannot be lowered at all (Darwin processes inherit virtual mappings that exceed the cap before `exec`, so `pre_exec` fails every spawn with `EINVAL`).

Teardown sweeps the server's process group while the leader is still unreaped: `stop` waits for the leader's exit with `waitid(WNOWAIT)`, signals the group, and reaps the leader last. The unreaped leader keeps its pid, which is also the group id, reserved, so the sweep can never reach an unrelated process that received a recycled pid.

Availability probes (for example `rust-analyzer --version`, which a rustup proxy may turn into a toolchain install) are bounded at 3 s. On timeout the probe's process group **and** every descendant found through parent links are frozen and SIGKILLed, so a child that called `setsid` does not escape.

### Verifying Linux-only paths

The `/proc` parsing and tree walk in `crates/engine/src/process.rs` are platform-independent and unit-tested on every host against a synthetic `/proc` directory (hostile `comm` fields, vanished and zombie pids, cycles, deep and oversized trees); only the `/proc` path and `sysconf` call are Linux-gated. The Linux `cfg` code itself (the `/proc` readers, the watchdog, their tests) can be compile- and lint-checked from macOS with [`cargo-zigbuild`](https://github.com/rust-cross/cargo-zigbuild) and zig, no extra C flags needed:

```sh
rustup target add aarch64-unknown-linux-gnu
cargo-zigbuild clippy -p octocode-engine --all-targets --all-features --target aarch64-unknown-linux-gnu -- -D warnings
cargo-zigbuild test -p octocode-engine --all-features --target aarch64-unknown-linux-gnu --lib --no-run
cargo-zigbuild clippy -p octocode-native --no-default-features --all-targets --target aarch64-unknown-linux-gnu -- -D warnings
```

Running the Linux tests (kernel `/proc`, address-space reservations past the cap, the RSS watchdog, SIGSTOP/SIGKILL of a `setsid` grandchild) still needs a real Linux kernel; run `cargo test -p octocode-engine --all-features` on a Linux host (CI's `native` job runs it on `ubuntu-latest`).

`start` and `stop` take the client's locks in one order (`child` → `connection` → `stderr_task`) and hold `child` throughout, so a `stop` that overlaps a `start` waits for it and then shuts down the server it published.

## Readiness and diagnostics

For a slow `lspSearch` call, enable `lsp.trace` (`OCTOCODE_LSP_TRACE=true`) through
the normal [configuration flow](../../../../docs/CONFIGURATION.md). Each executed
query row emits a diagnostic summary on stderr; CLI warm-server calls relay it,
and MCP keeps it outside JSON-RPC results. Disable the setting when finished.
Tracing does not change readiness gates, retry budgets, or semantic evidence.

The summary separates pool acquisition, process spawn, initialization, startup
readiness, document synchronization, first-open readiness, semantic requests, and
write-queue admission. Durations are cumulative and inclusive: acquisition can
contain initialization/readiness, and concurrent requests overlap, so **do not
add them to calculate wall-clock latency**. Only phases that ran are listed.
Completed/failed counts describe function returns, not index completeness; a
readiness wait can complete with the existing `timeout` readiness value. Dropped
futures count as cancelled. `active` identifies work still running when the
summary is emitted, including supervised startup after caller cancellation.

Traces contain phase labels, durations and counts only—not file paths, method
arguments, source text, environment values, or credentials. The collector has a
fixed set of phase keys and is isolated per query; tracing is not a request log.
The timing and race regression tests use in-memory servers and paused Tokio time.

Timed-out and dropped requests emit `$/cancelRequest` when possible. A request waits 30 s; past that it keeps waiting in 7.5 s steps while the server shows progress (CPU time of its process tree grew by at least 1% of a step since the last check, or a `$/progress` work-done token is open), up to a ceiling (content-modified retries of one request share it), so a cold server on a starved machine (CI, parallel agents) answers instead of timing out while it is still indexing. An lspSearch request's ceiling is its tool call's execution deadline (`DEFAULT_TIMEOUT`, 300 s, or the host's `timeout_secs`) less a 1 s reserve for the failure row: a server that keeps progressing may use all the time the call still has. A wait no call bounds (`initialize` inside the pooled start, prewarm) keeps a 60 s ceiling. A server with neither signal is hung and times out at 30 s either way. Cancelling the call, or the runtime at its deadline, drops the pending lspSearch work at once, which cancels its server requests. CPU time comes from `libproc` on macOS, `/proc` on Linux and `GetProcessTimes` on Windows (the server process only); elsewhere only `$/progress` extends the wait. The timeout message names the time actually waited. One request that uses its whole wait is cancelled and the server is kept. The connection is retired (and the pool starts a replacement on the next acquisition) on a second consecutive timeout with no answered request between, or when the cancel cannot be written.

Progress-aware servers must reach a full quiet interval before readiness is confirmed. New progress restarts that interval. Servers without progress use a bounded settle state (2 s at startup), which does not prove indexing completion. A first `didOpen` waits a short settle (400 ms) for a project load the open starts; it is skipped when the startup wait already confirmed the project load (`progressIdle`), and only progress already under way is drained. Empty results with unconfirmed readiness remain partial.

Pull-capable servers use `textDocument/diagnostic`. Push diagnostics are retained in a bounded per-document cache; content updates clear stale entries, versions reject older publications, and notifications for another document cannot satisfy a waiter.

Anchored navigation records available diagnostic health in `lsp.analysisContext`.
An empty answer waits up to one second for diagnostics; a nonempty answer uses
already published diagnostics without adding a pull request. Errors anywhere in
the document can degrade semantic coverage, including errors outside the queried
line. `noErrorsReported` describes the received report, not compilation success;
`unavailable` and an incomplete diagnostic report remain distinct. A diagnostic
follow-up preserves the query's workspace and Rust context.

C-family rows also list configuration candidates found in ancestor locations.
`configuration:"found"` records file presence; `command:"unverified"` explicitly
leaves the effective compiler command and indexed scope unverified. Missing
configuration and reported errors supply separate coverage warnings.

Semantic results are sorted and deduplicated before pagination. Continuations bind to the query, server identity, workspace/configuration fingerprints, and result snapshot. A changed result set produces a restart rather than combining pages from different semantic states.

## Rust context

The public `rustContext` fields map natively to rust-analyzer initialization options. They control Cargo features, default features, target, cfg values, build scripts, and procedural macros. Procedural macros require build scripts. Enabling either permits workspace code execution by rust-analyzer; Octocode does not sandbox that execution.

Effective Rust context participates in pool and continuation identity, so clients with different build configurations cannot share semantic state.
