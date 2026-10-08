# @octocodeai/octocode-chrome-devtools

Chrome MCP server and CLI for DOM actions, snapshots, arbitrary CDP methods/events, worker/frame sessions, network captures, profiling, heap/trace/PDF streams and complete saved evidence. The separate `octocode-chrome-devtools` skill is guidance only.

## Build and local use

Requires Chrome and Node 24.15+. This package is private; it has not been published. From the monorepo:

```sh
yarn workspace @octocodeai/octocode-chrome-devtools build
node /absolute/repo/packages/octocode-chrome-devtools/bin/octocode-chrome-devtools.mjs /cli --help
```

Run from the workspace cwd. A local package install/link exposes `octocode-chrome-devtools`. Built npm archives include the bundled runtime and shared config; no runtime dependency installation is needed. `src/` owns the TypeScript adapter and browser engines; `dist/` contains generated runtime code. `tests/` and `tools/` are development-only.

## One package, two interfaces

```sh
octocode-chrome-devtools                      # MCP stdio
octocode-chrome-devtools /cli --help          # typed CLI
octocode-chrome-devtools /cli skill --json    # complete operating guide
octocode-chrome-devtools /cli cdp --help --json
```

Like the communication package, default execution serves MCP and `/cli` runs typed commands using `octocode-mcp-cli`. `/raw` exposes the original argument-oriented engine CLI for advanced/script integration. Both interfaces share one command spec and the same execution engine. Package exports provide `runChromeMcp` at the root, `runChromeCli` at `/cli`, and the spec/runtime at `/runtime`.

Configure an MCP host with command `octocode-chrome-devtools` and no arguments, setting its cwd to the workspace. Without an installed bin, use `node` with the absolute package bin path. Core tools expose structured plans, steps, methods, recipes and connections. Specialized helpers take existing CLI arguments in `args`. See [setup and input formats](docs/mcp-cli.md) and [OPERATING.md](OPERATING.md), also served through `skill`.

Capture artifacts and state live in `<cwd>/.octocode/tmp/chrome-devtools/`. Replies carry direct read pages and compact action acknowledgements in `flow`, with lossless routes to complete step records, artifact inventories and logs. Shared source identities are hoisted into `flow.sources`; relative continuations resolve from the workspace cwd. Use `--preset research` for a ten-tool MCP catalog. Use Octocode `localSearch`/`localFetch` on returned capture scopes and indexed `query` for structured rows; filter before paging. Every withheld result remains reachable through `next.*` continuations, including oversized values. Calls serialize within one server. MCP cancellation stops active execution and rejects queued calls before they start. Partial captures survive; inspect state before retrying uncertain mutations.

## Verification

```sh
yarn workspace @octocodeai/octocode-chrome-devtools verify
yarn workspace @octocodeai/octocode-chrome-devtools test:live
yarn workspace @octocodeai/octocode-chrome-devtools test:webmcp
```

`verify` builds, typechecks all TypeScript sources (strict checking at the shared boundaries), checks tooling syntax and the guidance-only skill boundary, runs the hermetic groups, then runs the same advanced local Chrome fixtures through raw CLI, MCP and typed CLI. It also packs and extracts the package outside the monorepo, launches isolated Chrome, executes the documented research capture through both transports, verifies a typed CLI CDP call and checks a synthetic cookie round trip. Run `test:package` for that archive smoke test alone. `check:build` detects stale or obsolete generated engine/adapter/config outputs.

The protocol suite inventories every advertised command/event separately from executed methods. Representative fixtures cover DOM/CSS/accessibility, trusted input, workers, interception, debugger/CPU profiling, heap chunks, trace/PDF streams, WebSockets, storage, emulation, large plans, readiness and fail-stop evidence. Structural dispatch is not proof that every operation works in every target, permission or Chrome version. Use `--only <name>`, `--transport legacy|mcp|typed-cli` and `--report /absolute/coverage.json` for focused verification.

[Architecture and ownership](ARCHITECTURE.md) describes implementation boundaries. Optional scraping bridges require the separate scraping skill; use `--scraping-skill-dir /absolute/skill` when it cannot be discovered.
