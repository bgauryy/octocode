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
```

The pager returns small pages of metadata and points back to the HAR for full evidence.

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

## 4. Browser-discover to API/curl replay

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

## Playwright vs CDP quick rule

- Use these CDP examples for live forensics, manual browsing, console/network/perf evidence, and current DOM state.
- Use Playwright for maintained tests, locators/assertions/retries, cross-browser checks, `recordHar`, and `routeFromHAR` replay.
- Hybrid: debug with CDP, save HAR/summary artifacts, then promote stable flows into Playwright/API fixtures.

## Token strategy

- Print only `[METRIC]`, `[FINDING]`, `[NETWORK_ERROR]`, `[EXCEPTION]`, and `[ARTIFACT]` lines.
- Write raw/high-volume data to files.
- Page large HAR files with `har-pager.mjs` instead of pasting the whole HAR into chat.
- Use bounded monitor durations; rerun for another window rather than leaving a script unbounded.
