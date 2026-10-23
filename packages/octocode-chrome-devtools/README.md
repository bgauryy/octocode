# @octocodeai/octocode-chrome-devtools

Chrome MCP server and CLI for DOM actions, snapshots, arbitrary CDP methods/events, worker/frame sessions, network captures, profiling, heap/trace/PDF streams and complete saved evidence. The npm archive bundles the complete guidance-only `octocode-chrome-devtools` Agent Skill under `dist/skills/octocode-chrome-devtools/`, including references and report scripts. The canonical source is `skills/octocode-chrome-devtools` in the monorepo; the build refreshes the bundled copy. This package also ships `OPERATING.md` and its topic guides, served by `/cli skill`; that command reads guidance and does not install Agent Skills.

## Build and local use

Requires Chrome and Node.js 24.15.0 or later in the 24.x line (`^24.15.0`). Run from the workspace cwd:

```sh
npx -y @octocodeai/octocode-chrome-devtools /cli --help
```

From the monorepo:

```sh
yarn workspace @octocodeai/octocode-chrome-devtools build
node /absolute/repo/packages/octocode-chrome-devtools/bin/octocode-chrome-devtools.mjs /cli --help
```

An install or local link exposes `octocode-chrome-devtools`. Built npm archives include the bundled runtime and shared config; no runtime dependency installation is needed. `src/` owns the TypeScript adapter and browser engines; `dist/` contains generated runtime code. `tests/` and `tools/` are development-only.

## Bundled Agent Skill

After installing the package, locate the skill entry with:

```sh
node -p "require.resolve('@octocodeai/octocode-chrome-devtools/skill')"
```

Copy the complete containing `octocode-chrome-devtools` directory into your agent's skill directory; the references and report scripts must stay beside `SKILL.md`. Running through `npx` includes the same files in its package cache but does not register a skill with your agent. Alternatively, use `octocode skill install octocode-chrome-devtools` to install from the Octocode skill catalog.

## One package, two interfaces

```sh
octocode-chrome-devtools                      # MCP stdio
octocode-chrome-devtools /cli --help          # typed CLI
octocode-chrome-devtools /cli skill --json    # operating guide; follow any continuation
octocode-chrome-devtools /cli cdp --help --json
```

Like the communication package, default execution serves MCP and `/cli` runs typed commands using `octocode-mcp-cli`. `/raw` exposes the original argument-oriented engine CLI for advanced/script integration. Both interfaces share one command spec and the same execution engine. Package exports provide `runChromeMcp` at the root, `runChromeCli` at `/cli`, and the spec/runtime at `/runtime`.

Configure an MCP host with command `octocode-chrome-devtools` and no arguments, setting its cwd to the workspace. Without an installed bin, use `node` with the absolute package bin path. Core tools expose structured plans, steps, methods, recipes and connections. Specialized helpers take existing CLI arguments in `args`. See [setup and input formats](docs/mcp-cli.md) and [OPERATING.md](OPERATING.md), also served through `skill`.

Capture artifacts and state live in `<cwd>/.octocode/octocode-chrome-devtools/tmp/`. Replies carry direct read pages and compact action acknowledgements in `flow`, with lossless routes to complete step records, artifact inventories and logs. Shared source identities are hoisted into `flow.sources`; relative continuations resolve from the workspace cwd. Use `--preset research` for a ten-tool MCP catalog. Use Octocode `localSearch`/`localFetch` on returned capture scopes and indexed `query` for structured rows; filter before paging. Every withheld result remains reachable through `next.*` continuations, including oversized values. Calls serialize within one server. MCP cancellation stops active execution and rejects queued calls before they start. Partial captures survive; retries follow [bounded retry](docs/recovery.md#bounded-retry).

## Verification

```sh
yarn workspace @octocodeai/octocode-chrome-devtools verify
yarn workspace @octocodeai/octocode-chrome-devtools test:live
```

`verify` builds, typechecks all TypeScript sources (strict checking at the shared boundaries), checks tooling syntax and the guidance-only skill boundary, runs the hermetic groups, then runs the same advanced local Chrome fixtures through raw CLI, MCP and typed CLI. It also verifies offline npx execution from the packed archive and checks every bundled skill file against its canonical source. The live archive smoke test extracts the package outside the monorepo, launches isolated Chrome, executes the documented research capture through both transports, verifies a typed CLI CDP call and checks a synthetic cookie round trip. `test:webmcp` is part of `verify`: it launches a headless Chrome with the WebMCP feature and checks tool listing and invocation on a local fixture (about 8 s). Run `test:package` for that archive smoke test alone. `build` rewrites every generated engine/adapter/config output and deletes obsolete ones.

The protocol suite plan-validates every advertised command/event offline (`planValidatedCommands`) and reports the methods its fixtures send to Chrome (`testedMethods`). Representative fixtures cover DOM/CSS/accessibility, trusted input, workers, interception, debugger/CPU profiling, heap chunks, trace/PDF streams, WebSockets, storage, emulation, large plans, readiness and fail-stop evidence. Use `--only <name>`, `--transport legacy|mcp|typed-cli` and `--report /absolute/coverage.json` for focused verification.

`test:cdp-all` (part of `verify`) sends every command the installed Chrome advertises to a disposable isolated headless browser (page target first; the browser, tab and dedicated-worker targets then take what the previous one answered `-32601`, and each row names the `target` that answered and the targets `tried`), with params built only from the schema (required fields, type placeholders, first enum value, resolved `$ref`). Each reply is classified: `ok`, `chrome-error` (Chrome routed and rejected the call, with its code), `exception`, `timeout`, `target-destroyed`/`browser-destroyed` (loss reproduced by replay; the suite replaces the target or relaunches and continues), `unavailable` (-32601 on every target for an experimental or deprecated member), `not-found` (-32601 on every target for a stable member), `transport` (unreproduced loss) and `engine`. The last three fail. It also registers a `listen` step for every advertised event. `--via script` (default, about 25 s) sends the queue over one sandboxed `script` connection; `--via run` sends batched `run` plans of `cdp` steps, resuming after each stop (about 2.5 min, one engine call per stop). Use `--only <Domain>` and `--report <file>`; a timeout or a reply shaped by placeholder params is evidence of routing, not of the command's semantics.

[Architecture and ownership](ARCHITECTURE.md) describes implementation boundaries. Optional scraping bridges require the separate scraping skill; use `--scraping-skill-dir /absolute/skill` when it cannot be discovered.
