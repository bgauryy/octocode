# CDP Automation Intents

Load for page actions, scraping, or live-page attachment. Why: prevent accidental navigation or mutation.

## Smart Automation Rules
1. Lock target, trigger, signals, evidence prefixes before scripting.
2. Enable domains, attach listeners, then act — never reverse that order.
3. Prefer `--new-tab about:blank` + `Page.navigate` inside `run()` for load evidence.
4. Wait for visible/enabled selectors before fill/click. Fill via the prototype native setter, not instance — see `references/script-patterns-async.md#waitForSelector` (`examples/dom-operations-check.mjs` already does this).
5. Prefer one meaningful change per iteration when mutating the page; reuse `--port` and `--keep-tab`. Read-only checks may cover several fields in one pass.
6. Split broad work into small scripts; do not build one giant audit unless asked.
7. On live pages: no navigation unless requested; read current DOM/storage/perf via `Runtime.evaluate`.

## automate
Prefer snapshot before selector: run `examples/page-snapshot.mjs` first, then target elements with `DOM_REF=<ref>` on `examples/dom-operations-check.mjs` instead of guessing a CSS selector. Refs are backed by `backendDOMNodeId` — stable within the page, invalid after navigation. Click/fill/submit only when requested. Record each step as `[ACTION]`; a successful one also prints `[CODE] locator=... action=...` for promoting into a maintained test suite.

Known sequence (fill → click → check one result): write one custom `run(cdp)` script covering all of it instead of one `cdp-sandbox.mjs` invocation per step — each invocation is a separate process with its own fixed overhead, so N single-action calls cost roughly N× that for no extra correctness. Reserve the per-step example-script pattern for exploratory work where the next action depends on what the last one revealed. To confirm an action's effect, check the one element/state that should have changed (a targeted `Runtime.evaluate` or a single `dom-operations-check.mjs` inspect) instead of a whole new page-snapshot — a full re-snapshot costs as much as the original discovery pass to confirm a single fact.

## scrape
Read structured data without mutation. Emit counts and sample rows; page large output to files. For broad public crawls, first use `octocode-scraping` to build a corpus/graph; use CDP here to validate graph action nodes (search inputs, next buttons, menus, infinite scroll) and return discovered URLs/data to that corpus.

## live-page
Attach with `--keep-tab`. Listeners miss past events — re-read current state.

## webmcp
Only when the user explicitly asks about page-native/AI tools, an agent-callable API on the page, or WebMCP by name — not by default before `automate`/`scrape`. It needs a fresh Chrome launch (`--enableFeatures WebMCP`, no reusing the current session), so probing it unprompted adds a real relaunch cost for a domain almost no site has adopted yet. When it does apply: structured JSON in/out instead of selector guessing, same trust boundary as a click — page code still runs with page privileges.

1. Launch with `--enableFeatures WebMCP` (Chrome 150+; existing/reused sessions can't add this — start a fresh port). See `references/chrome-flags.md`.
2. Run `examples/webmcp-tools.mjs` with `WEBMCP_ACTION=list`. `[WEBMCP_TOOL]` lines mean the page opted in; `[FINDING] WEBMCP_NO_TOOLS` is the common case today — fall back to `automate`/`scrape` instead of retrying.
3. To call a discovered tool: `WEBMCP_ACTION=invoke WEBMCP_TOOL=<name> WEBMCP_INPUT='<json matching inputSchema>'`. Check the tool's `risk=` annotation in the list output first; treat `risk=mutating` the same as a real click under the Mutation Gate below.
4. Read `[WEBMCP_RESULT] status=...` — `Completed`, `Error`, `Canceled`, or this script's own `Timeout` guard. Full payload lands in `webmcp-invocation.json`.

WebMCP is an experimental (`tot`) CDP domain — `[FINDING] WEBMCP_NO_TOOLS` is the expected outcome even when the user did ask for it.

## Mutation Gate
Ask before purchases, sends, deletes, account changes, or submitting real user data.

Next: waits in `references/script-patterns-async.md`; shadow DOM/uploads in `references/script-patterns-browser.md`.
