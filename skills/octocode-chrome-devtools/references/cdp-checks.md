# CDP check scripts (`scripts/cdp-checks/`)

Invoke via `cdp-sandbox.mjs` (or `cdp-runner.mjs`) on a live `--port`.

| Script | Role |
|---|---|
| `page-snapshot` / `dom-operations-check` | A11y refs; inspect/click/fill + `[CODE]` |
| `graph-actionability-check` / `actionability-diagnostics` | Graph actions; classify zero rows |
| `performance-` / `network-` / `storage-measure-check` | Smart health 0–100 + JSON (`MEASURE_URL` / `MEASURE_EXISTING=1`) |
| `storage-cookies-audit` | Legacy counts-only — prefer storage-measure |
| `measure-query` | Filter measure JSON (`--view/--code/--kind/--domain/--latest`) |
| `live-har-monitor` / `network-body-har-fetch-check` | Deep HAR/bodies after measure+query |
| `har-pager` / `har-redact` | Page `.har` (`--filter/--kind/--status/--url-regex`); redact before share |
| `api-replay` / `stealth-check` / `affiliates-stealth-probe` | Replay; stealth smoke |
| `webmcp-tools` (+ `.check`) | WebMCP list/invoke + hermetic grader |

## Measure → query (no re-browser)
```bash
# trio on same tab
node <skill>/scripts/cdp-sandbox.mjs <skill>/scripts/cdp-checks/performance-measure-check.mjs --port 9222 --keep-tab
MEASURE_EXISTING=1 node …/network-measure-check.mjs --port 9222 --keep-tab
MEASURE_EXISTING=1 node …/storage-measure-check.mjs --port 9222 --keep-tab
# query
node …/measure-query.mjs --latest --view findings --code HTTP_FAILURES
node …/har-pager.mjs <file.har> --filter failures --format json   # standalone HAR
node …/corpus-run-local.mjs --artifact-dir <run> --regex 'offerId' --limit 20
```

| Data | Tool |
|---|---|
| perf/net/storage JSON | `measure-query` |
| Standalone `.har` | `har-pager` (not measure-query unless same run dir + `--har`) |
| Corpus / any artifact | `corpus-run-local` |

Graders: `eval-*-measure.mjs`, `eval-measure-query.mjs`. Chain: snapshot → DOM → measure → query → (optional) HAR → ingest. See `har-capture.md`, `intents-debug.md`.
