Load when reading a session's layout, choosing what to search first, running a bounded search, or interpreting its files.

# Corpus layout and search

## Layout

```text
.octocode/octocode-scraping/tmp/{sessionId}/
  AGENT_INDEX.json   # read first
  README.md, manifest.json, MAP.md, page-map.json, sources.jsonl
  indexes/pages-NNN.json, indexes/top-links.jsonl, indexes/workflow-candidates.jsonl
  graph/site-graph.json, graph/graph.json, graph/workflows.json, schemas/graph.schema.json
  pages/, text/*.clean.part-*.md, extracts/, snippets/, reports/ (summary.md, failures.md, robots.md)
  raw/               # audit only
  cdp/, extracts/cdp-*.jsonl, extracts/bridge-handoff.json
```

- Query the corpus; never dump pages into chat.
- Each completed page has a private `.pending/page-NNN.json` checkpoint until all session indexes are written. After interruption, `--append` recovers these pages and their link frontier without overwriting their evidence. Uncommitted page files produce `sessionIncomplete` ([what to do](recovery.md#failures)). Writes use atomic file replacement; concurrent writers to one session and power-loss durability are not supported.

## Search order

1. `AGENT_INDEX.json` (`schemaVersion`, warnings, thinHints, `bridge-handoff.json`); stop on warnings unless partial evidence is acceptable.
2. `indexes/` (page past the first only if needed) and `graph/` candidates.
3. `cdp/` and `extracts/cdp-*.jsonl` before thin `text/*.clean.part-*.md`.
4. `corpus-run` or local search over the candidates.
5. The exact file for citation (after a bridge, `cdp/<ingestId>-body-NNN.txt`); `raw/` only to audit extraction.

- Join a multi-part page: `corpus-run.mjs --session-dir <d> --regex <p> --concat-parts --write-full-clean`.

## Bounded search

- `corpus-find.mjs --session-dir <dir> --query <text> [--limit <page-size>] [--offset <offset>]`.
- Output: `matches`, `isPartial`, `completeness` (`partial` | `complete`), `pagination` (`offset`, `limit`, `totalMatches`, `returnedMatches`, `remainingMatches`, `hasMore`), `suggestedFiles`.
- Run `next.page` unchanged while rows remain; the end returns `next:null` and `completeness:"complete"`. Restart at offset 0 after a fetch.

## Corpus files

- `warnings`: provider errors, target error pages, truncation, partial evidence.
- `AGENT_INDEX.json` `analysis` points to indexes and `automationGraph`/`automationGraphSchema`; a bridge adds `cdpBridge`, `cdpNetwork`, `cdpBodies`, `bridgeHandoff`, and `cdp/`, `extracts/cdp-*.jsonl` to `searchTargets`.
- `cdp/` files per ingest id (`cdp-har-…`, `cdp-bodies-…`, `cdp-capture-…`): `<id>.redacted.har` (`<id>.raw.har` under `--no-redact`), `<id>-network-summary.json`, `<id>-network-bodies.json`, `<id>-body-NNN.txt`, redacted DOM captures.
- Re-ingesting replaces that capture's rows in `sources.jsonl` and `extracts/cdp-*.jsonl`.
- Redaction masks credentials by field name and token shape; `--no-redact` only for an explicit audit. Unredacted source files are never copied in.
