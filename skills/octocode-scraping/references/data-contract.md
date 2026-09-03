# Data Contract

Load when inspecting returned values, scripts, or corpus files. Why: stable file contracts let agents paginate and analyze deterministically.

## Stdout contract
Compact JSON only: `ok`, `sessionId`, `sessionDir`, `route`, `status`, `pages`, `warnings`, `agentIndex`, `analysis`, `searchFirst`, `rawAudit`. Never include scraped content.

Bridge helpers also emit compact JSON (never raw HAR/HTML):
- `har-ingest.mjs`: `ok`, `flow` (`cdp→scrape` | `scrape→cdp-packet`), `sessionDir`, `thinHints`, `cdpFiles`, `extracts`, `next`
- `corpus-run.mjs`: `ok`, `flow` (`local-iterate`), `sessionDir`/`artifactDir`, `matchCount`, `matches[]` (path/line/snippet), optional `script.result`

## Corpus contract
- Default folder: `.octocode/tmp/scrape/{sessionId}`.
- `schemaVersion` lives in `AGENT_INDEX.json`.
- `warnings` explain provider errors, target error pages, truncation risks, and partial evidence.
- `analysis` points to deterministic files: page index, site graph, top links, and `automationGraph`/`automationGraphSchema` (`graph/graph.json` validated against `schemas/graph.schema.json`) for external automations.
- After bridge process, `analysis` also includes `cdpBridge`, `cdpNetwork`, `cdpBodies`, `bridgeHandoff`; `searchTargets` includes `cdp/` and `extracts/cdp-*.jsonl`.
- `raw/` is optional audit data and excluded from first-pass search.
- `cdp/` holds redacted HAR, `network-summary.json`, `network-bodies.json`, and `body-*.txt` from chrome-devtools runs (same scrape `sessionId`).

## Corpus pagination contract
Large crawls must expose small index files under `indexes/`: `pages-001.json`, then `pages-002.json`, etc. Agents read page indexes before text chunks, then fetch exact files from `searchFirst`. Do not confuse this with a link's `workflowType: "pagination"` (`references/website-analysis.md`) — that means the *target site* has more pages, not this corpus.

## Quality checks
Verification must cover compact stdout, no raw payload stdout, the agent index, graph v2, source evidence on graph nodes and edges, target-error warnings, and secret rejection without a stack trace. It must also cover cost capture, failure reports, and resource extraction that never carries a `workflowType`. The bridge path must verify thinHints, redacted HAR processing, and API-field proof from a local regex or script without live Chrome.

Next: to walk the folder and search order load `references/session-corpus.md`; to judge the extracted data load `references/extraction-quality.md`.
