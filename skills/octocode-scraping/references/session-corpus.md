# Session Corpus

Load after any fetch/crawl/extract or before searching saved outputs. Why: web content becomes a normalized local corpus that Octocode tools can search/read/prove without context bloat.

## Layout
```text
.octocode/tmp/scrape/{sessionId}/
  AGENT_INDEX.json          # read first: schema, warnings, totals, search targets
  manifest.json
  MAP.md / page-map.json    # URL ↔ file mapping
  sources.jsonl             # URL/status/content-type/fetch metadata
  indexes/pages-001.json    # paginated page rows
  indexes/top-links.jsonl   # ranked navigation candidates
  graph/graph.json          # portable node/edge automation graph
  graph/site-graph.json     # richer site/workflow graph
  graph/workflows.json
  schemas/graph.schema.json
  pages/{pageId}.json
  text/{pageId}.md
  text/{pageId}.clean.part-001.md
  extracts/{metadata,headings,links,resources,costs}.jsonl|json
  extracts/{pageId}-ai-extract.json
  snippets/{pageId}-*.txt
  reports/{summary,crawl-summary,costs,failures}.md
  raw/{pageId}.html|json    # audit/debug only
```

## Proof loop
```text
fetch/crawl/extract
→ .octocode/tmp/scrape/{sessionId}/ + AGENT_INDEX.json
→ Octocode local tools over indexes, graph, clean chunks, extracts
→ exact source citations without raw context dumps
```

## Search order
1. `localViewStructure` on the session folder.
2. Read `AGENT_INDEX.json`; stop or qualify if warnings/targetLikelyError matter.
3. Read `indexes/pages-001.json` and `graph/site-graph.json` for candidate pages, pagination, and workflow paths.
4. `localSearchCode` in `reports/`, `text/*.clean.part-*.md`, `extracts/`, `indexes/`, `graph/`, `snippets/`, `manifest.json`, and `sources.jsonl`.
5. `localGetFileContent` compact files for exact evidence; read `raw/` only to audit extraction or debug missing data.

## Anti-bloat contract
- Stdout is compact JSON: session path and next search targets, never scraped content.
- Raw payloads are preserved for audit but skipped in first-pass search.
- Clean text is chunked; extracts/reports carry metadata, links, headings, costs, failures, and samples for smart search.

## Citation pattern
Cite local evidence (`text/`, `extracts/`, `snippets/`) plus source metadata (`sources.jsonl` or `MAP.md`) for URL/status/fetch time.
