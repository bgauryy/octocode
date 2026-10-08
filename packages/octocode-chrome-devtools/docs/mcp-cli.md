# Package MCP and typed CLI

Load when setting up the Chrome package or using structured tool inputs. Why: MCP and CLI share one spec and engine through `octocode-mcp-cli`.

The package executable starts MCP by default; `/cli` selects the typed CLI and `/raw` selects the original argument-oriented engine CLI. Set the host server cwd to the workspace. The built package contains its bundled adapter, browser scripts, shared config and operating guides. The separate skill contains no implementation.

```json
{"mcpServers":{"chrome":{"command":"octocode-chrome-devtools","args":[]}}}
```

Without a linked bin, use command `node` and args `["/absolute/package/bin/octocode-chrome-devtools.mjs"]`. This package is private and not published; build/install its local folder. It requires Chrome and Node 24.15+.

The default MCP server exports all 25 commands. Start with `--preset research` to expose ten tools: open, cleanup, targets, run, snapshot, query, artifact, schema, protocol and skill. Both surfaces use schemas and instructions authored in the shared core contract. `run` takes `{plan, connection}`; `step` takes `{step, connection}`; `cdp` takes `{method, params, connection}`. `protocol` takes optional `{member, connection}`. Snapshot/screenshot accept `{options, connection}` and `check` adds `recipe`. `schema` takes optional `command`, `recipe`, and `operation`; a recipe requires `command: "check"`. `skill` takes optional `topic`, `offset`, and `length` (1–20000), and reads complete package guidance. `open` accepts `headless`, `port` and `url`; `cleanup` accepts `port` and `dryRun`; `query` accepts `file`, `format`, `pointer`, `where`, `select`, `cursor`, `limit` and `sha256`; `artifact` accepts `file`, `format`, `pointer`, `offset`, `length` and `sha256`. Open, cleanup, query and artifact also accept legacy `args`, but cannot mix it with typed fields. Other helpers take `args`, an array of existing CLI arguments passed without a shell.

Connection fields use camelCase: `target`, `targetUrl`, `targetType`, `newTab`, `browser`, `port`, `closeTab`, `keepTab`, `noReload`, `stealth`, `noStealth`, `timeout`, `scriptTimeout`, `verbose`, `dryRun`. Discover exact inputs from tool schemas or typed CLI help.

For operation details, call `schema` with `command: "run"` or `"step"` and `operation`, such as `"extract"`. The focused response includes an operation example, applicable frame scopes, and supported extraction fields: `text`, `href`, `value`, `role`, and `name`. It returns command input examples from shared core. `name` reads aria-label or the element name attribute, without computing an accessible name. An ambiguous frame URL error includes every candidate target. Choose one connection target selector: `target`, `targetUrl`, `newTab`, or `browser: true`. Deadlines use milliseconds. Conflicting target selectors, keep/close flags, and stealth flags fail before execution.

```sh
octocode-chrome-devtools /cli cdp --help --json
octocode-chrome-devtools /cli cdp --method Runtime.evaluate --params '{"expression":"document.title","returnByValue":true}' --connection '{"target":"<id>"}' --json
octocode-chrome-devtools /cli run --input /absolute/input.json --json
octocode-chrome-devtools /cli skill --topic browser-execution --json
octocode-chrome-devtools /cli schema --command check --recipe page-snapshot --json
octocode-chrome-devtools /cli schema --command run --operation extract --json
octocode-chrome-devtools /cli cleanup --port 9222 --dry-run true --json
```

Whole-input files/stdin contain the tool input object: `{"plan":{"steps":[{"op":"cdp","method":"Runtime.enable"}]},"connection":{"target":"<id>"}}`. Use `--input -` for stdin and large plans. Inline typed `--plan` takes a JSON object; `/raw run --plan` takes a file containing the plan alone. Detailed raw-engine examples set `CDP=/absolute/package/dist/engine/cli.mjs` and use `node "$CDP"`.

Success returns `structuredContent` with `ok`, `exitCode`, and a complete `data` page, a compact browser `flow` page, or a `capture` manifest. Browser actions return completion counts and verified postconditions without input-event payloads or echoed action records. `flow.steps` lists reads, postconditions and failures; `next.steps` lists every original step. `flow.events` reports listener counts and boundaries. Small CDP results and the first extraction page appear directly in `flow.steps`; oversized data and later steps have continuations. `flow.sources` hoists shared URL/frame identities; step timestamps and source indexes preserve provenance. `flow.coverage` states executed and unvisited steps and the capture boundary. `next.steps` gives the complete original step records, including artifact paths and action evidence. Capture replies mark `isPartial:true`: original results, findings and logs remain in saved files. `next.artifacts` pages the full artifact inventory; `next.capture` reads all manifest details. JSON reader pages appear directly in `data`, without escaped JSON/log wrappers. Oversized values and later pages have executable continuations; source digests prevent mixing captures. Errors set MCP `isError`; the text block contains the same JSON status/evidence envelope and complete error text or a paged log path. The typed CLI preserves the engine exit status; choose `--json` for machine-readable success.

Package continuations are `{tool, query}`. Relative continuation and search paths resolve from the workspace cwd; keep that cwd for CLI continuations. Absolute source paths remain in full captures. In MCP call `{name: next.tool, arguments: next.query}`. In the typed CLI save `next.query` to a file and run `<next.tool> --input file --json`, or pass its `args` through `--args`. The first complete artifact page appears in `artifacts`: resolve those paths against `root`. Only remaining rows have `next.artifacts`; subsequent indexed rows carry full paths. Returned `search.paths` are search scope metadata: choose a task anchor and use Octocode `localSearch` with those paths and the returned flags; it supplies its own complete pages and read continuations. `hints.localFetch` is an executable Octocode lead. Run Octocode from the same workspace so its allowed-root boundary includes captures. Native Octocode tools take their own query envelope, not the browser helper `args` input.

Calls serialize within one server. Separate servers must coordinate a shared tab. An MCP cancellation notification aborts active engine execution and rejects queued calls before they launch. Captured partial logs survive cancellation; an in-flight mutation remains uncertain. Explicit engine deadlines also bound calls. Inspect uncertain mutation outcomes before retrying.

Maintainers use `yarn workspace @octocodeai/octocode-chrome-devtools build`, `check:build`, and `verify`. The build bundles `octocode-mcp-cli` once and refreshes shared config; the extracted archive needs no runtime dependencies. The protocol suite's `--transport legacy|mcp|typed-cli` runs the same real-browser assertions through all interfaces. Extracted-package tests check actual npm archive contents and stdio operation outside the repository.

Next: steps, event references and sessions → [browser execution](browser-execution.md); failed target/request → [recovery](recovery.md).

For repeated calls, keep one MCP client connected to the same server and workspace, then close it when the task ends. Reconnecting for each call repeats server initialization. Calls still serialize, and each browser operation runs its bounded engine; a persistent client does not create a browser daemon or relax cancellation and ownership boundaries.

Typed query predicates reject numeric integers outside the safe JavaScript integer range. Use an integer string such as `"9007199254740993"` for exact comparisons. Raw helper predicate JSON preserves original numeric lexemes. Returned numeric-filter continuations use legacy `args` to preserve those lexemes; copy the whole continuation unchanged.

Maintainers can reproduce client/page-size comparisons with `node packages/octocode-chrome-devtools/tools/persistent-client-benchmark.mjs <absolute-result-directory>` from the repository root. It records the frozen contract, exact calls, complete evidence digests, guardrail results, setup/operation timing and runtime fingerprints. This scripted comparison measures the specified workload; autonomous quality and billed usage require separate evaluation.
