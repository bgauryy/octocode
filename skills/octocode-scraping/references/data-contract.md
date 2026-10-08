# Data contract and extraction quality

Load when inspecting returned values, scripts, or corpus files, or when turning fetched pages into facts, rows, or summaries. Stable file contracts let agents paginate and analyze deterministically; scraped data is noisy and must stay auditable. Why: retain complete evidence and identify transformed data.

## Stdout contract

Compact JSON only: `ok`, `sessionId`, `sessionDir`, `route`, `status`, `pages`, `warnings`, `agentIndex`, `analysis`, `searchFirst`, `rawAudit`. Never include scraped content. Bridge helpers also emit compact JSON (never raw HAR/HTML):
- `har-ingest.mjs`: `ok`, `flow` (`cdp→scrape` | `scrape→cdp-packet`), `sessionDir`, `thinHints`, `cdpFiles`, `extracts`, `next`
- `corpus-run.mjs`: `ok`, `flow` (`local-iterate`), `sessionDir`/`artifactDir`, `matchCount`, `matches[]` (`file`/`abs`/`line`/`column`/`match`/`snippet`), `next[]` (`file`/`line`/`match`), optional `script.result`; `--regex` is a JavaScript regex (`--flags i`, not `(?i)`)

## Corpus contract

- Default folder: `.octocode/tmp/scrape/{sessionId}`. `schemaVersion` lives in `AGENT_INDEX.json`.
- `warnings` explain provider errors, target error pages, truncation risks, and partial evidence.
- `analysis` points to deterministic files: page index, site graph, top links, and `automationGraph`/`automationGraphSchema` for external automations.
- After a bridge, `analysis` also includes `cdpBridge`, `cdpNetwork`, `cdpBodies`, `bridgeHandoff`; `searchTargets` includes `cdp/` and `extracts/cdp-*.jsonl`.
- `raw/` is optional audit data, excluded from first-pass search. `cdp/` holds redacted HAR, `network-summary.json`, `network-bodies.json`, and `body-*.txt` from chrome-devtools runs (same scrape `sessionId`).

## Extraction quality

- Before extraction, define the target schema or claim list. Record route, source URL, status, content type, and fetch time in `sources.jsonl`.
- Emit counts and 3–5 sample rows for structured extraction. Spot-check critical facts against raw HTML or source snippets; cross-check important claims with another selector or source when possible.
- Mark partial, blocked, transformed, or inferred data explicitly. Markdown transformation and AI summaries are not authoritative without exact source evidence.
- Store rows as JSONL in `extracts/`, compact evidence excerpts in `snippets/`, and the narrative in `reports/summary.md`.

## Quality checks

Verification must cover compact stdout, no raw payload stdout, the agent index, graph v2, source evidence on graph nodes and edges, target-error warnings, and secret rejection without a stack trace. It must also cover cost capture, failure reports, and resource extraction that never carries a `workflowType`. The bridge path must verify thinHints, redacted HAR processing, and API-field proof from a local regex or script without live Chrome.

Next: to walk the folder and search order load `references/session-corpus.md`.

Downloaded raw bodies are retained in full unless `--no-raw` opts out. Clean text is chunked without clipping or changing UTF-8 characters. `--max-text-bytes` bounds each clean-text part together with `--chunk-bytes`; it does not discard remaining downloaded text. Direct/hosted HTTP retains the existing disclosed terminal network cap of max(raw limit, text limit, 64000). Increasing that cap changes collection size.

Query lists use executable `next.*` commands, corpus fingerprints and bounded pages. `source-query.mjs` returns independently decodable base64 byte pages; concatenate decoded bytes before interpreting UTF-8 text. Regex scans report oversized/unreadable files explicitly and provide source continuations. Ranking scans all stored text parts and stores per-term counts instead of page bodies. Graph construction and query metadata still load graph/roster data in memory.
