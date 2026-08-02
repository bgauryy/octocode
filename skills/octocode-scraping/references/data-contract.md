# Data Contract

Load when inspecting returned values, scripts, or corpus files. Why: stable file contracts let agents paginate and analyze deterministically.

## Stdout contract
Compact JSON only: `ok`, `sessionId`, `sessionDir`, `route`, `status`, `pages`, `warnings`, `agentIndex`, `analysis`, `searchFirst`, `rawAudit`. Never include scraped content.

## Corpus contract
- Default folder: `.octocode/tmp/scrape/{sessionId}`.
- `schemaVersion` lives in `AGENT_INDEX.json`.
- `warnings` explain provider errors, target error pages, truncation risks, and partial evidence.
- `analysis` points to deterministic files: page index, site graph, top links, and `automationGraph`/`automationGraphSchema` (`graph/graph.json` validated against `schemas/graph.schema.json`) for external automations.
- `raw/` is optional audit data and excluded from first-pass search.

## Corpus pagination contract
Large crawls should expose small index files under `indexes/`: `pages-001.json`, then `pages-002.json`, etc. Agents read page indexes before text chunks, then fetch exact files from `searchFirst`. Do not confuse this with a link's `workflowType: "pagination"` (`references/website-analysis.md`) — that means the *target site* has more pages, not this corpus.

## Quality checks
Eval must verify: compact stdout, no raw payload stdout, agent index exists, graph v2 exists, graph nodes/edges carry source evidence, target-error warnings, secret rejection without stack trace, cost capture, failure reports, and resource extraction never carries a `workflowType`.
