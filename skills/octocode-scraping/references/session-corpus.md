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
1. `AGENT_INDEX.json` (warnings / thinHints / `bridge-handoff.json`); stop on warnings unless partial evidence is acceptable
2. `indexes/` (paginate only when the target is not on the first page) + `graph/` candidates
3. If present, `cdp/` + `extracts/cdp-*.jsonl` before thin `text/*.clean.part-*.md`
4. `corpus-run` / local search on reports, text, extracts, cdp, indexes, graph, snippets, sources, after choosing candidate pages/links
5. Exact file for citation; `raw/` only to audit extraction

## Bounded search

`corpus-find.mjs --session-dir <dir> --query <text> [--limit 20] [--offset 0]` returns one ranked page. Bounds must be safe integers: limit is positive and offset is non-negative. Missing values, duplicate or unknown options, and invalid bounds return exit 2 with `ok:false` and `error.code:"invalidArguments"`.

Successful output keeps `matches` and adds `isPartial`, `completeness` (`partial` or `complete`), and `pagination` with `offset`, `limit`, `totalMatches`, `returnedMatches`, `remainingMatches`, and `hasMore`. While rows remain, `next.page` contains an absolute `command` and raw `args`; execute them unchanged to continue. The final page, an empty search, and an offset at or beyond the result count return `next:null` and `completeness:"complete"`. Suggested evidence paths are in `suggestedFiles`, separate from executable continuations.

Pages preserve descending score order, including stable input order for ties. Keep the corpus unchanged while paging; after a fetch or corpus update, restart at offset 0. Each call reads the current corpus rather than persisting a snapshot.

After a bridge, cite `cdp/body-*.txt`. Concat parts: `corpus-run --concat-parts --write-full-clean`.

Next: for each file's field contract load `references/data-contract.md`; to navigate the graph load `references/website-analysis.md`; if the corpus is thin or blocked load `references/failure-recovery.md`.
