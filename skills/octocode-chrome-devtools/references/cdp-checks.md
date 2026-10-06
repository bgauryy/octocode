# Ready checks (`scripts/cdp-checks/`)

Load when running a check, reading its artifacts, or capturing HAR. Why: Choose a check and read its full artifacts. Artifacts: `.octocode/tmp/chrome-devtools/<timestamp>/`, printed as `[ARTIFACT]`.

| Check | Does | Knobs |
|---|---|---|
| `page-snapshot` | `[PAGE]` title+URL, refs `e1…` (controls, headings; document order; same-process iframes inlined; isolated frames print `FRAME_TARGET` under `— iframe <url>`; `clickable` = div/span with cursor:pointer, onclick or tabindex), `—` row/card context lines, `~"…"` inferred names for unnamed controls. All refs saved to `page-snapshot.json`; `[NEXT]` when more | `SNAPSHOT_MAX` (60/page), `SNAPSHOT_PAGE=n`, `SNAPSHOT_OUTLINE=1` (regions `rN` + headings with ref spans), `SNAPSHOT_ROOT=<css\|eN\|rN>`, `SNAPSHOT_VIEWPORT=1`, `SNAPSHOT_TEXT=<n>` (+`page-text.txt`), `SNAPSHOT_CONTEXT=0`, `SNAPSHOT_URLS=1` (link paths), `SNAPSHOT_CLICKABLE=0`, `SNAPSHOT_STDOUT=summary`, `SNAPSHOT_WAIT_SELECTOR`, `SNAPSHOT_WAIT_TEXT`, `SNAPSHOT_WAIT_MS` |
| `page-screenshot` | JPEG of viewport (1920×1080 under stealth), full page (≤8000px per tile; complete paginated manifest), or one element | `SHOT_FULL=1`, `SHOT_SELECTOR=<css>`, `SHOT_SCALE` (0.25–1; 0.5 ≈ ¼ bytes), `SHOT_FORMAT=png`, `SHOT_QUALITY` (70), `SHOT_ANNOTATE=1` (box + label the snapshot refs; `e20-e80` for a range) |
| `dom-operations-check` | Actionability, real (trusted) mouse/keyboard input aimed by `DOM.getContentQuads` (correct inside iframes; re-aimed if layout shifts), then `[VERIFY]`: value/checked/selected/files read-back, navigation, DOM mutations, focus, dialogs, popups; `[DIFF]`/`[NEW] [eN]` names refs the action revealed (menus, dialogs) and saves a new `page-snapshot.json` without changing the original capture. Recovers stale refs by unique role+name in the target realm | `DOM_REF`, `DOM_SELECTOR`, or exact `DOM_ROLE` + `DOM_NAME` (ambiguous matches fail); `DOM_ACTION=inspect\|click\|dblclick\|fill\|type\|press\|hover\|select\|check\|uncheck\|focus\|scroll\|upload\|drag\|wait`; `DOM_VALUE` (text, option label/value, scroll px, upload `/abs/a\|/abs/b`, wait text `a\|b`); `DOM_TO_REF`/`DOM_TO_SELECTOR` (drag target; pointer and HTML5); `DOM_STEPS='[{"ref":"e2","action":"fill","value":"x"},…]'` (batch, stops at first failure); `DOM_WAIT_TEXT` + `DOM_WAIT_MS` (8000); `DOM_ACTION=wait DOM_SELECTOR=<css>` waits for visible content; `DOM_DIFF=0`; `DOM_TRACE_EVENTS=1` (types, trust, target and timing; no typed keys/data); `DOM_KEY` (`Enter`, `Tab`, `Escape`, `ArrowDown`, `Control+a`…); `DOM_INPUT=js` (synthetic fallback); `DOM_SETTLE_MS` (500); `DOM_DIALOG=accept` (default dismiss) |
| `graph-actionability-check` | Operable rows `{url, rows[]}` | `--url`, `--selectors`, `--limit` (25 stdout rows; full set saved), `--graph <file>` |
| `actionability-diagnostics` | Classifies empty pages: blocked, js-shell, consent-region… | `--url`, `--wait-ms` |
| `performance-` / `network-` / `storage-measure-check` | Health 0–100 + findings JSON | `MEASURE_URL=<url>` (fresh load, bounded observation window) or `MEASURE_EXISTING=1` (current tab; network sees only new requests). Neither = built-in fixture. Perf reports LCP (+element), CLS, and `interactionLatencyMax` after an interaction (maximum observed event duration); `inputHandlerMax` records delay plus handler time. These are diagnostics, not field INP. The tab is brought to front and a tiny screencast keeps headless painting, so late LCP is timed correctly. `PERF_WAIT_MS`, `NET_WAIT_MS`, `NET_SLOW_MS`, `STORAGE_WAIT_MS` |
| `protocol-snapshot` | Save the installed Chrome schema for every domain, command, event and type | Run through sandbox; page with `artifact-query` |
| `artifact-query` | Lossless bounded pages of any saved capture | `--file`, `--format text\|json\|binary`, `--pointer /path`, `--offset`, `--length` (1–20000); SHA-256-pinned continuations; encoded content ≤20000 bytes |
| `snapshot-query` | Page saved refs or outline without recapturing dynamic content | `--file`, `--page`, `--limit`, `--view refs\|outline` |
| `measure-query` | Filter latest measure JSON, no browser | `--latest`, `--view`, `--page`, `--limit`, `--code`, `--kind`, `--domain`, `--min-ms`, `--har-file` |
| `live-har-monitor` | HAR + timing, console errors; no bodies | `MONITOR_URL` (load after listeners attach; else passive on the current tab), `MONITOR_MS` (30000), `SLOW_MS`, `MAX_STDOUT_ITEMS` |
| `network-body-har-fetch-check` | HAR + response bodies → `network-bodies.json` | `BODY_URL=<page>` (else fixture), `BODY_MATCH=<url substring>` (default XHR/Fetch/JSON), `BODY_WAIT_MS` (3000); all matching completed bodies are retained |
| `har-pager` / `har-redact` | Page a `.har`; redact before sharing | `--filter all\|failures\|slow\|domain:<host>`, `--min-ms`, `--kind`, `--status`, `--url-regex`, `--page`; `--keep-bodies` (manual review), `--strip-bodies`, `--out` |
| `api-replay` | Replay one request without Chrome | `--url`, `--method`, `--headers`, `--body`, `--response-file`, `--page`, `--max-chars` (1–20000), `--timeout-ms` |
| `stealth-check` / `affiliates-stealth-check` | Stealth self-test plus the detector page's own verdicts (`DETECTOR_FAILED`) | `--stealth`, `STEALTH_CHECK_URL`, `AFFILIATES_CHECK_URL` |
| `webmcp-tools` (+ `.check`) | WebMCP list/invoke; `.check` launches its own Chrome and grades 8 cases | `WEBMCP_ACTION`, `WEBMCP_TOOL`, `WEBMCP_INPUT`, `WEBMCP_FRAME`, `WEBMCP_WAIT_MS` |

Scripts without the sandbox (`artifact-query`, `snapshot-query`, `measure-query`, `har-*`, `api-replay`, `webmcp-tools.check`) run with plain `node`.

## Page health → query

```bash
for c in performance network storage; do
  MEASURE_URL="<url>" node $S/cdp-sandbox.mjs $S/cdp-checks/$c-measure-check.mjs --port 9222 --keep-tab
done
node $S/cdp-checks/measure-query.mjs --latest --view findings
node $S/cdp-checks/har-pager.mjs <run>/live-network.har --filter failures --format json
```

## HAR rules

- Choose measures for page health, a monitor for an observation window, and bodies for response-content questions.
- Keep full evidence in artifacts. Page HAR and measure lists with their executable `next.continue`; follow all relevant pages before making completeness claims.
- `Network.getResponseBody` works after `loadingFinished` and only while the body is cached; an attached tab misses past bodies, so load the page inside the check (`BODY_URL`).
- Network checks enable event domains in related isolated iframe sessions. Coverage lists attached frames, errors and any pending requests. Initial iframe requests can precede attachment; an attached target also misses past traffic. WebSockets need the websocket intent.
- Share only `har-redact` output. In a scraping bridge for thin pages, trust API bodies over rendered text.

Next: no check fits → `script-patterns.md`; error or empty output → `recovery.md`.

Measurements report unavailable data explicitly; a missing evaluation or cookie inventory cannot earn a healthy score. Performance scores are heuristics, and observed interaction maxima are not field INP. API replay continuations read a saved response; they never repeat the request. HAR redaction strips request and response bodies by default; `--keep-bodies` requires manual review before sharing.

A document reaching `interactive` allows inspection; it does not prove SPA content is ready. Wait for task-specific text or a selector. Mutating DOM actions poll missing, hidden, disabled, covered or moving controls until `DOM_WAIT_MS`, with `[PROGRESS]` updates. Exact role/name lookup uses the accessibility tree, including shadow controls. After navigation, refresh refs. For an isolated iframe, list targets and pass `--target <id>` (or `--target-type iframe --target-url <url>`); its snapshot and actions use the frame’s own realm. Network duration includes body completion, redirect hops stay separate, and unfinished requests stay in artifacts.
