# CDP Automation Intents

Load for page actions, scraping, or live-page attachment. Why: prevent accidental navigation or mutation.

## Smart Automation Rules
1. Lock target, trigger, signals, evidence prefixes before scripting.
2. Enable domains, attach listeners, then act — never reverse that order.
3. Prefer `--new-tab about:blank` + `Page.navigate` inside `run()` for load evidence.
4. Wait for visible/enabled selectors; dispatch framework-visible `input`/`change` events.
5. One meaningful change per iteration; reuse `--port` and `--keep-tab`.
6. Split broad work into small scripts; do not build one giant audit unless asked.
7. On live pages: no navigation unless requested; read current DOM/storage/perf via `Runtime.evaluate`.

## automate
Click/fill/submit only when requested. Record each step as `[ACTION]`.

## scrape
Read structured data without mutation. Emit counts and sample rows; page large output to files.

## live-page
Attach with `--keep-tab`. Listeners miss past events — re-read current state.

## webmcp
Try before falling back to `automate`/`scrape` when the page might expose page-native tools. Prefer this path: it gives structured JSON in/out instead of selector guessing, and it's the same trust boundary as a click — page code still runs with page privileges.

1. Launch with `--enableFeatures WebMCP` (Chrome 150+; existing/reused sessions can't add this — start a fresh port). See `references/chrome-flags.md`.
2. Run `examples/webmcp-tools.mjs` with `WEBMCP_ACTION=list`. `[WEBMCP_TOOL]` lines mean the page opted in; `[FINDING] WEBMCP_NO_TOOLS` is the common case today — fall back to `automate`/`scrape` instead of retrying.
3. To call a discovered tool: `WEBMCP_ACTION=invoke WEBMCP_TOOL=<name> WEBMCP_INPUT='<json matching inputSchema>'`. Check the tool's `risk=` annotation in the list output first; treat `risk=mutating` the same as a real click under the Mutation Gate below.
4. Read `[WEBMCP_RESULT] status=...` — `Completed`, `Error`, `Canceled`, or this script's own `Timeout` guard. Full payload lands in `webmcp-invocation.json`.

WebMCP is an experimental (`tot`) CDP domain with low site adoption — treat a positive discovery as the exception, not the default path.

## Mutation Gate
Ask before purchases, sends, deletes, account changes, or submitting real user data.

Next: waits in `references/script-patterns-async.md`; shadow DOM/uploads in `references/script-patterns-browser.md`.
