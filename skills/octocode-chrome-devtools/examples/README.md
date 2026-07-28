# Chrome DevTools Examples

These examples are runnable with the skill sandbox and are designed to keep agent output small while writing full evidence to files.

## 1. Open a visible browser and let the user interact

```bash
node skills/octocode-chrome-devtools/scripts/open-browser.mjs \
  --url "https://example.com" \
  --port 9222
```

Use the page normally. In another terminal, attach a bounded monitor to the same tab:

```bash
node skills/octocode-chrome-devtools/scripts/cdp-sandbox.mjs \
  skills/octocode-chrome-devtools/examples/live-har-monitor.mjs \
  --port 9222 \
  --target-url "example.com" \
  --keep-tab \
  --timeout 60000 \
  --script-timeout 70000
```

The monitor does not navigate. It records new network/console/runtime events while you interact.

Artifacts are written under the sandbox output directory printed by `[ARTIFACT]` lines:

- `live-network.har` — HAR 1.2-shaped HTTP archive for requests seen during the run.
- `events.ndjson` — one event per line for streaming/diff review.
- `network-summary.json` — compact counts, slow requests, failures, and page timing.

## 2. Page through a HAR without loading it all into context

```bash
node skills/octocode-chrome-devtools/examples/har-pager.mjs \
  .octocode/chrome-devtools/<run>/live-network.har \
  --page 1 \
  --page-size 25
```

Useful follow-ups:

```bash
# only failures
node skills/octocode-chrome-devtools/examples/har-pager.mjs live-network.har --filter failures

# only slow entries
node skills/octocode-chrome-devtools/examples/har-pager.mjs live-network.har --filter slow --min-ms 1000

# compact JSON for agents
node skills/octocode-chrome-devtools/examples/har-pager.mjs live-network.har --format json --page-size 10

# redact before sharing
node skills/octocode-chrome-devtools/examples/har-redact.mjs live-network.har --strip-bodies
```

The pager returns small pages of metadata and points back to the HAR for full evidence.

## 2b. Cookie bridge (opt-in, never prints values)

```bash
node skills/octocode-chrome-devtools/scripts/cookie-bridge.mjs --i-understand-secrets \
  --from-port 9333 --to-port 9222 --urls "https://example.com"
```

Prefer `--from-storage-state` or `--from-port` over `--from-profile` when Chrome is already open.

## 3. Check and operate on DOM elements

```bash
node skills/octocode-chrome-devtools/scripts/cdp-sandbox.mjs \
  skills/octocode-chrome-devtools/examples/dom-operations-check.mjs \
  --port 9222 \
  --target-url "example.com" \
  --keep-tab
```

Configure via environment variables:

```bash
DOM_SELECTOR="button[type=submit]" DOM_ACTION=click \
node skills/octocode-chrome-devtools/scripts/cdp-sandbox.mjs \
  skills/octocode-chrome-devtools/examples/dom-operations-check.mjs \
  --port 9222 --target-url "example.com" --keep-tab
```

Supported `DOM_ACTION` values: `inspect`, `click`, `fill`. For fill, set `DOM_VALUE`.

The DOM example reports only bounded facts: visibility, disabled state, hit-test coverage, accessibility name/role, stable bounding box, and shadow-aware selector path. It writes `dom-check.json` for full structured details.

## 4. Validate scrape graph actionability

After `octocode-scraping` creates `graph/graph.json`, validate candidate action nodes in a live browser:

```bash
node skills/octocode-chrome-devtools/scripts/cdp-sandbox.mjs \
  skills/octocode-chrome-devtools/examples/graph-actionability-check.mjs \
  --port 9222 \
  --target-url "example.com" \
  --keep-tab \
  --graph .octocode/tmp/scrape/<session>/graph/graph.json
```

Writes `graph-actionability.json` and emits `[ACTIONABILITY]` rows with visible/enabled/stable/covered/accessibility facts.

## 5. Diagnose zero actionability rows

When a scraping graph has action nodes but CDP actionability returns 0 rows, run:

```bash
node skills/octocode-chrome-devtools/scripts/cdp-sandbox.mjs \
  skills/octocode-chrome-devtools/examples/actionability-diagnostics.mjs \
  --port 9222 --new-tab "https://example.com" --timeout 60000
```

It classifies likely causes: blocked, JS-shell, selector-mismatch, consent-region, or timing-hydration; writes JSON plus a screenshot for proof.

## 6. Cookie and storage audit

```bash
node skills/octocode-chrome-devtools/scripts/cdp-sandbox.mjs \
  skills/octocode-chrome-devtools/examples/storage-cookies-audit.mjs \
  --port 9222 --target-url "example.com" --keep-tab
```

Writes `storage-cookies-audit.json`. It prints cookie metadata and storage key counts only — never cookie/localStorage values.

## 7. Build a local CDP protocol corpus

```bash
node skills/octocode-chrome-devtools/scripts/protocol-corpus.mjs --out .octocode/cdp-protocol
```

Uses `octocode-scraping` to fetch protocol domain docs so agents can search exact current CDP methods locally.

## 8. HAR + response body capture with request interception

`network-body-har-fetch-check.mjs` demonstrates `Fetch.enable` before navigation, fulfills a mocked API response, captures `Network.getResponseBody`, and writes both HAR and body artifacts.

```bash
node skills/octocode-chrome-devtools/scripts/cdp-sandbox.mjs \
  skills/octocode-chrome-devtools/examples/network-body-har-fetch-check.mjs \
  --port 9222 --new-tab about:blank
```

Use this pattern for API discovery: intercept or observe safely, capture body immediately, write HAR/body files, then analyze them with Octocode local tools instead of pasting payloads.

## 9. Browser-discover to API/curl replay

For public data flows, first use browser/network evidence to identify the request shape, then prefer a documented endpoint or direct HTTP replay over DOM scraping.

```bash
node skills/octocode-chrome-devtools/examples/api-replay.mjs \
  --url "https://example.com/api/items?page=1" \
  --headers '{"accept":"application/json"}' \
  --max-chars 4000
```

Equivalent curl shape:

```bash
curl -s -H "accept: application/json" "https://example.com/api/items?page=1"
```

Use the browser only when UI behavior matters. For data returned by an endpoint, replay the request with non-secret headers and page the response instead of scraping brittle DOM text.

## 10. Stealth check before scraping a bot-walled site

```bash
node skills/octocode-chrome-devtools/scripts/cdp-sandbox.mjs \
  skills/octocode-chrome-devtools/examples/stealth-check.mjs \
  --port 9222 --new-tab "about:blank" --timeout 30000
```

Applies `scripts/undercover.mjs`'s stealth patches, navigates to `STEALTH_CHECK_URL` (default: `bot.sannysoft.com`), then self-tests with `verifyStealth`. Writes `stealth-check.json` with the full per-signal breakdown. Run this before scraping a site likely to fingerprint headless Chrome — a `MOSTLY_CLEAN`/`DETECTED` result means switch to a visible user gate instead.

## 11. Discover and invoke page-declared WebMCP tools

Requires Chrome 150+ launched fresh with the feature flag — an already-running/reused session can't add it:

```bash
node skills/octocode-chrome-devtools/scripts/open-browser.mjs \
  --headless --port 9222 --enableFeatures WebMCP --url "https://example.com"
```

List whatever the page has registered via `document.modelContext.registerTool(...)`:

```bash
WEBMCP_ACTION=list node skills/octocode-chrome-devtools/scripts/cdp-sandbox.mjs \
  skills/octocode-chrome-devtools/examples/webmcp-tools.mjs \
  --port 9222 --target-url "example.com" --keep-tab
```

`[FINDING] WEBMCP_NO_TOOLS` is the expected result on most sites today — adoption is still low. When tools are found, check the `risk=` field on each `[WEBMCP_TOOL]` line, then invoke one by name with JSON input matching its `inputSchema`:

```bash
WEBMCP_ACTION=invoke WEBMCP_TOOL=<tool-name> WEBMCP_INPUT='{"...":"..."}' \
node skills/octocode-chrome-devtools/scripts/cdp-sandbox.mjs \
  skills/octocode-chrome-devtools/examples/webmcp-tools.mjs \
  --port 9222 --target-url "example.com" --keep-tab
```

`risk=mutating` tools fall under the same Mutation Gate as a real click; only invoke those on explicit request. Full tool list and invocation payloads land in `webmcp-tools.json`/`webmcp-invocation.json` under the sandbox output dir.

### Self-check (skill maintainers, not general page use)

`examples/webmcp-tools.check.mjs` is a deterministic TDD grader, not a page-inspection tool: it launches an isolated Chrome against the bundled fixture (`examples/fixtures/webmcp-fixture.html`, which registers a real `echo_price` tool) and asserts discovery, invocation, and the no-tools fallback all behave correctly. Run it after touching `webmcp-tools.mjs`, `open-browser.mjs`, or `cdp-sandbox.mjs`:

```bash
node skills/octocode-chrome-devtools/examples/webmcp-tools.check.mjs --port 9245
```

Exit 0 and `[PASS]` on all lines means the intent still works end to end; any `[FAIL]` is a real regression, not a flaky page.

## Playwright vs CDP quick rule

- Use these CDP examples for live forensics, manual browsing, console/network/perf evidence, and current DOM state.
- Use Playwright for maintained tests, locators/assertions/retries, cross-browser checks, `recordHar`, and `routeFromHAR` replay.
- Hybrid: debug with CDP, save HAR/summary artifacts, then promote stable flows into Playwright/API fixtures.

## Token strategy

- Print only `[METRIC]`, `[FINDING]`, `[NETWORK_ERROR]`, `[EXCEPTION]`, and `[ARTIFACT]` lines.
- Write raw/high-volume data to files.
- Page large HAR files with `har-pager.mjs` instead of pasting the whole HAR into chat.
- Use bounded monitor durations; rerun for another window rather than leaving a script unbounded.
