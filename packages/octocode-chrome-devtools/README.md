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
```

`verify` builds, typechecks all TypeScript sources (strict checking at the shared boundaries), checks tooling syntax and the guidance-only skill boundary, and runs the browser-free unit and self-tests (`test`). `build` rewrites every generated engine/adapter/config output and deletes obsolete ones.

### Browser checks

Real-browser behavior is checked by an agent, by hand, judged by reading the replies (method: [tool checks](../octocode-benchmark/CHECKS.md)). Use an isolated headless Chrome, a temporary cwd and local pages served from a loopback port; no network, no real profile. Run each item through MCP, the typed `/cli` and the `/raw` engine CLI, and compare the three.

- [ ] Snapshot and act: `snapshot` returns refs for a page's controls (including a same-process iframe and an isolated frame's `FRAME_TARGET`); `fill`, `click`, `select`, `check`, `upload`, `drag` and `press` on those refs change the page, and `[VERIFY]` reads the change back. A stale ref is recovered by role and name, or refused when ambiguous.
- [ ] Wait and scope: a wait for text or a selector returns once it appears and fails with evidence when it does not; text scoped to a region excludes the rest of the page.
- [ ] Capture: a full-page screenshot pages through its tiles; a `browser-execute` plan stops at the first failing step and keeps the partial artifacts; every withheld value is reachable through its `next.*` continuation and `artifact-query` pages are lossless.
- [ ] Measure: `performance-`, `network-` and `storage-measure-check` on a slow local page report its LCP, slow requests and storage, with findings that match what the page does.
- [ ] Protocol: a `run` plan of `cdp` steps reaches Chrome for DOM/CSS/accessibility, trusted input, interception, workers, profiling, trace and PDF streams, WebSockets, storage and emulation, and returns Chrome's own error for a bad call. `/raw schema` lists the installed Chrome's domains; sending a few commands from several domains with schema-built params gives `ok` or Chrome's error, never a lost transport.
- [ ] Cleanup: after the session ends, no Chrome or helper process it started remains, including one started from another cwd.
- [ ] WebMCP: a fresh headless Chrome (not a reused session; `open` enables WebMCP) opens a local page that registers one tool. `webmcp-tools` list mode finds it, invoke mode returns its structured output with status `Completed`, and on a page with no tools both exit 0 with an empty result, not an error or a hang.
- [ ] Persistent client: the same reads through one connected MCP client and through a reconnect per call return identical evidence; the connected client skips the repeated initialization.
- [ ] Packed archive: `npm pack --ignore-scripts`. The archive holds `bin/`, `dist/{runtime,cli,mcp}.js`, the config loader, `OPERATING.md`, `docs/` and the bundled skill under `dist/skills/`, and nothing under `src/`, `tools/`, `tests/` or `node_modules/`. Outside the monorepo and offline, `npm exec --package <tgz> -- octocode-chrome-devtools /cli cdp --help --json` prints a schema with `method`, `/raw schema run` lists the plan operations, and the MCP server lists `skill`, whose `operating` and `web-research` topics, paged to the end, equal `OPERATING.md` and `docs/web-research.md` byte for byte. The documented research capture works through MCP and the typed CLI, a typed CDP call answers, and a synthetic cookie round-trips.

[Architecture and ownership](ARCHITECTURE.md) describes implementation boundaries. Optional scraping bridges require the separate scraping skill; use `--scraping-skill-dir /absolute/skill` when it cannot be discovered.
