# Ready checks (`scripts/cdp-checks/`)

Load when running a check, reading its artifacts, or capturing HAR. Run through `cdp-sandbox.mjs … --port <n> --keep-tab`; one at a time per port. Artifacts: `.octocode/tmp/chrome-devtools/<timestamp>/`, printed as `[ARTIFACT]`.

| Check | Does | Knobs |
|---|---|---|
| `page-snapshot` | `[PAGE]` title+URL, refs `e1…` (controls, headings; document order), `—` row/card context lines, `~"…"` inferred names for unnamed controls. All refs saved to `page-snapshot.json`; `[NEXT]` when more | `SNAPSHOT_MAX` (60/page), `SNAPSHOT_PAGE=n`, `SNAPSHOT_OUTLINE=1` (regions `rN` + headings with ref spans), `SNAPSHOT_ROOT=<css\|eN\|rN>`, `SNAPSHOT_VIEWPORT=1`, `SNAPSHOT_TEXT=<n>` (+`page-text.txt`), `SNAPSHOT_CONTEXT=0`, `SNAPSHOT_STDOUT=summary` |
| `page-screenshot` | JPEG of viewport (1920×1080 under stealth), full page (≤8000px), or one element | `SHOT_FULL=1`, `SHOT_SELECTOR=<css>`, `SHOT_SCALE` (0.25–1; 0.5 ≈ ¼ bytes), `SHOT_FORMAT=png`, `SHOT_QUALITY` (70) |
| `dom-operations-check` | Actionability, real (trusted) mouse/keyboard input, then `[VERIFY]`: value/checked/selected read-back, navigation, DOM mutations, focus, dialogs, popups. Recovers stale refs by role+name | `DOM_REF` or `DOM_SELECTOR`; `DOM_ACTION=inspect\|click\|fill\|type\|press\|hover\|select\|check\|uncheck\|focus\|scroll`; `DOM_VALUE` (text, option label/value, scroll px); `DOM_KEY` (`Enter`, `Tab`, `Escape`, `ArrowDown`, `Control+a`…); `DOM_INPUT=js` (synthetic fallback); `DOM_SETTLE_MS` (400); `DOM_DIALOG=accept` (default dismiss) |
| `graph-actionability-check` | Operable rows `{url, rows[]}` | `--url`, `--selectors`, `--limit` (25), `--graph <file>` |
| `actionability-diagnostics` | Classifies empty pages: blocked, js-shell, consent-region… | `--url`, `--wait-ms` |
| `performance-` / `network-` / `storage-measure-check` | Health 0–100 + findings JSON | `MEASURE_URL=<url>` (fresh load, full capture) or `MEASURE_EXISTING=1` (current tab; network sees only new requests). Neither = built-in fixture. Perf reports LCP (+element), CLS, and INP after an interaction (input delay + handler; headless paint timing is excluded). `PERF_WAIT_MS`, `NET_WAIT_MS`, `NET_SLOW_MS`, `STORAGE_WAIT_MS` |
| `measure-query` | Filter latest measure JSON, no browser | `--latest`, `--view`, `--code`, `--kind`, `--domain`, `--min-ms`, `--har-file` |
| `live-har-monitor` | HAR + timing, console errors; no bodies | `MONITOR_URL` (load after listeners attach; else passive on the current tab), `MONITOR_MS` (30000), `SLOW_MS`, `MAX_STDOUT_ITEMS` |
| `network-body-har-fetch-check` | HAR + response bodies → `network-bodies.json` | `BODY_URL=<page>` (else fixture), `BODY_MATCH=<url substring>` (default XHR/Fetch/JSON), `BODY_WAIT_MS` (3000); max 50 bodies |
| `har-pager` / `har-redact` | Page a `.har`; redact before sharing | `--filter all\|failures\|slow\|domain:<host>`, `--min-ms`, `--kind`, `--status`, `--url-regex`, `--page`; `--strip-bodies`, `--out` |
| `api-replay` | Replay one request without Chrome | `--url`, `--method`, `--headers`, `--body`, `--page`, `--max-chars` (500–20000) |
| `stealth-check` / `affiliates-stealth-check` | Stealth self-test plus the detector page's own verdicts (`DETECTOR_FAILED`) | `STEALTH_CHECK_URL`, `AFFILIATES_CHECK_URL` |
| `storage-cookies-audit` | Legacy counts for all browser cookies; prefer storage-measure | — |
| `webmcp-tools` (+ `.check`) | WebMCP list/invoke; `.check` launches its own Chrome and grades 8 cases | `WEBMCP_ACTION`, `WEBMCP_TOOL`, `WEBMCP_INPUT`, `WEBMCP_FRAME`, `WEBMCP_WAIT_MS` |

Scripts without the sandbox (`measure-query`, `har-*`, `api-replay`, `webmcp-tools.check`) run with plain `node`.

## Page health → query

```bash
for c in performance network storage; do
  MEASURE_URL="<url>" node $S/cdp-sandbox.mjs $S/cdp-checks/$c-measure-check.mjs --port 9222 --keep-tab
done
node $S/cdp-checks/measure-query.mjs --latest --view findings
node $S/cdp-checks/har-pager.mjs <run>/live-network.har --filter failures --format json
```

## HAR rules

- Measure + query first; long monitors and bodies only when they still decide something.
- Stdout stays at counts + `[ARTIFACT]`; summaries under 2 KB, pages of 10–50 rows. Never paste a whole HAR or judge from its first page.
- `Network.getResponseBody` works after `loadingFinished` and only while the body is cached; an attached tab misses past bodies, so load the page inside the check (`BODY_URL`).
- HAR covers HTTP(S) in all frames; WebSockets need the websocket intent.
- Share only `har-redact` output. Scrape bridge into an existing scraping session: `har-ingest-to-scrape.mjs --session-dir <s> --from-cdp-dir <run>` (or `--har <file>`) → `corpus-run-local.mjs --artifact-dir <run> --regex <re>`; for thin pages, trust API bodies over rendered text.

Next: no check fits → `script-patterns.md`; error or empty output → `recovery.md`.
