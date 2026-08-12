# Session Corpus

Load after fetch/crawl/extract or before searching saved output. Why: normalized local corpus without dumping pages into chat.

## Layout
```text
.octocode/tmp/scrape/{sessionId}/
  AGENT_INDEX.json   # read first
  manifest.json · MAP.md · page-map.json · sources.jsonl
  indexes/ · graph/ · schemas/graph.schema.json
  pages/ · text/*.clean.part-*.md · extracts/ · snippets/ · reports/
  raw/               # audit only
  cdp/ · extracts/cdp-*.jsonl · extracts/bridge-handoff.json
```

## Search order
1. `AGENT_INDEX.json` (warnings / thinHints / `bridge-handoff.json`)
2. `indexes/` + `graph/` candidates
3. If present, `cdp/` + `extracts/cdp-*.jsonl` before thin `text/*.clean.part-*.md`
4. `corpus-run` / local search on reports, text, extracts, cdp, indexes, graph, snippets, sources
5. Exact file for citation; `raw/` only to audit extraction

## Bridge
`har-ingest --from-cdp-dir` → `corpus-run --roots cdp,extracts --regex` — cite `cdp/body-*.txt`, skip re-browser. Reverse: `--export-packet` → chrome `graph-actionability-check`.

Stdout = session path + next targets, never scraped bodies. Concat parts: `corpus-run --concat-parts --write-full-clean`. Cite local path + `sources.jsonl` / `MAP.md` URL metadata.
