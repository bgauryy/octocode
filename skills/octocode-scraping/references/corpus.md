Load when reading script output, searching a session, mapping a site, or extracting facts.

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
- Each completed page has a private `.pending/page-NNN.json` checkpoint until all session indexes are written. After interruption, `--append` recovers these pages and their link frontier without overwriting their evidence. Uncommitted page files produce `sessionIncomplete`: preserve that directory and start a new session. Writes use atomic file replacement; concurrent writers to one session and power-loss durability are not supported.

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

## Stdout contract

- Capture commands return JSON metadata; query commands can return evidence snippets or source pages. `fetch.mjs` fields: `ok`, `sessionId`, `sessionDir`, `route`, `status`, `contentType`, `pages` (session total), `newPages` (this run), `antCreditsKnownTotal`, `creditCap` (with `--max-credits`: `spentThisRun`, `largestRequest`, `costUnknownAt`, `stopped`, `reason`, `unfetchedUrls`; else `null`), `providerDetail`, `providerRequest` (hosted endpoint and sent options, never the key or page URLs; `null` for direct and cdp), `warnings`, `next` (a `route` handoff such as `octocode-chrome-devtools`, or `null`), `robots`, `agentIndex`, `analysis`, `searchFirst`, `rawAudit`.
- Errors print `{"ok":false,"error":{"code","message"}}` on stdout: exit 2 for invalid input (`invalidArguments`, `sessionExists`, `refusedHost` for a private start URL); exit 1 for a runtime failure such as `sessionNotFound` (`--session-dir` is not a scrape session). Exception: `corpus-inspect`, `dom-find`, `graph-navigate`, `resource-list` print usage on stderr for a missing `--session-dir`; `corpus-inspect --format coverage-md` prints Markdown ([output](../output.md#saved-report)).
- An `--append` run that fetches no new page warns and exits 1.
- A refused crawled link, sitemap entry, or `cdp` request appears in `warnings` as `refused <url>: <reason>`; a refused redirect is that page's `error` in `sources.jsonl`.
- `har-ingest.mjs` prints `ok`, `flow` (`cdp→scrape` | `scrape→cdp-packet`), `sessionDir`, `thinHints`, `cdpFiles`, `extracts`, `next`; never raw HAR or HTML.
- `corpus-run.mjs` prints `ok`, `flow` (`local-iterate`), `sessionDir`/`artifactDir`, `matchCount`, `matches[]` (`file`/`abs`/`line`/`column`/`match`/`snippet`), `next[]` (`file`/`line`/`match`), optional `script.result`.
- `corpus-run.mjs` default roots: `text,extracts,cdp,snippets,reports,indexes,graph,sources`; add `raw` to audit. `--regex` is JavaScript: `--regex 'price' --flags i`, not `(?i)`.

## Corpus files

- `warnings`: provider errors, target error pages, truncation, partial evidence.
- `AGENT_INDEX.json` `analysis` points to indexes and `automationGraph`/`automationGraphSchema`; a bridge adds `cdpBridge`, `cdpNetwork`, `cdpBodies`, `bridgeHandoff`, and `cdp/`, `extracts/cdp-*.jsonl` to `searchTargets`.
- `cdp/` files per ingest id (`cdp-har-…`, `cdp-bodies-…`, `cdp-capture-…`): `<id>.redacted.har` (`<id>.raw.har` under `--no-redact`), `<id>-network-summary.json`, `<id>-network-bodies.json`, `<id>-body-NNN.txt`, redacted DOM captures.
- Re-ingesting replaces that capture's rows in `sources.jsonl` and `extracts/cdp-*.jsonl`.
- Redaction masks credentials by field name and token shape; `--no-redact` only for an explicit audit. Unredacted source files are never copied in.

## Site graph

- `graph/graph.json`: unified automation graph (nodes, typed edges, risks, confidence, evidence), shape in `schemas/graph.schema.json` (not validated at runtime); hand it to downstream tools. A repository test checks the producer against it.
- `graph/site-graph.json`: page detail; `indexes/pages-NNN.json` pages large crawls.
- `indexes/top-links.jsonl` ranks links; `workflowType: "pagination"` comes from `rel="next"/"prev"` or a `pagination`/`pager` class, never link text.
- `extracts/resources.jsonl`: assets (`script`, `stylesheet`, `image`, `media`, `feed`), no `workflowType`.
- Map a site with `graph-navigate.mjs --session-dir <d>`, not raw pages; for the whole site, judge the bounded crawl's graph before asking to expand.
- Choose the next pages from ranked links: `node <skill-folder>/scripts/resource-list.mjs --session-dir <d> --min-score 4 [--workflow <type>] [--query <text>]` lists links and assets best first. Each row's `score` and `reasons` (same-host, labeled, `workflow:<type>`, shallow; minus for same-page anchors, hash-only links, boilerplate labels and assets) come from the link scorer, which also writes them to `indexes/top-links.jsonl`; below 4 is noise. `facets` counts rows by kind and workflow type; run `next` while present. Judge labels and URLs against the task: the score only orders candidates.
- Every page of a listing: `node <skill-folder>/scripts/fetch.mjs --url <listing-url> --crawl --follow pagination --max-pages <n>` queues only `rel=next/prev` and pagination-class links (`--session <id> --append` extends a session); a warning says when no fetched page had one. Never guess URL patterns.
- Forms, buttons, tables, meta, JSON-LD, canonical links and code blocks: `node <skill-folder>/scripts/dom-find.mjs --session-dir <d> --kind table [--page-id <id>] [--query <text>]`. Each match carries `pageUrl`, `evidenceFile` (the extract holding it) and `textEvidence` (the page's first text part) to cite; `totalMatches` counts every page of `matches`.

## Extraction

- Define the schema or claim list first. `sources.jsonl` holds route, URL, status, content type, fetch time.
- Extract rows with code, not by reading pages: write an extractor module exporting `async function run(ctx)` that reads candidate files with `ctx.read(rel)` (`ctx.files` lists them), writes JSONL with `ctx.write('extracts/<name>.jsonl', text)` and returns `{ok, rows}`. Save it under `.octocode/tmp/octocode-scraping/` and run `node <skill-folder>/scripts/corpus-run.mjs --session-dir <d> --script <extractor.mjs> [--script-arg key=value]`. The output's `script` holds the return value (inline up to 4000 bytes, else its `next` continuation); a thrown error is `scriptFailed` (exit 1). Fix the extractor and rerun; report its row count and representative rows.
- Claim–evidence: each row carries `pageId`, URL, `file` and `line` (or the quoted span); spot-check critical rows against raw HTML or snippets and cross-check key claims with another selector or source.
- Facts, assumptions, unknowns: mark partial, blocked, transformed, or inferred data instead of presenting it as fact.
- Excerpts in `snippets/`; narrative in the saved report ([output](../output.md)), not `reports/summary.md`.

## Capture limits

- Response limits come from `--max-raw-bytes` and `--max-text-bytes`; inspect `fetch.mjs --help` and returned warnings rather than copying defaults into requests. Bodies are saved to `raw/` unless `--no-raw`. If `networkTruncated`, `rawTruncated` or `textTruncated` is set, raise the relevant limit or report unread scope.
- `--chunk-bytes` and `--max-text-bytes` size clean-text parts; nothing is discarded.
- [source-query.mjs](../scripts/source-query.mjs) pages one file; other scripts' continuations call it. Its default `--encoding auto` returns a UTF-8 file as text pages that end on character boundaries, so the pages' `content` in order is the file exactly; other bytes come back as base64 (`encoding` says which). Text pages of any file: `node <skill-folder>/scripts/source-query.mjs --file <path> --encoding utf8` (`notUtf8`, exit 2, on invalid UTF-8).

## Find evidence in unread text (classifiers, clasify)

- Admission and result rules belong to `octocode-research`; inspect the tool's live schema before calling it.
- Reuse the saved capture; narrow files by metadata, titles, URLs, and literal search. Skip empty and repeated captures by data: `node <skill-folder>/scripts/corpus-inspect.mjs --session-dir <d> --view captures` lists each capture's `bytes` and `sha256`; drop rows marked `empty` or carrying `duplicateOf` (`captureSummary` counts both).
- Log, HAR, or minified JSON: extract records as text first, keeping the original (positions are not DOM coordinates); read small ones directly.
- Which passages to read: `localSearch` over the capture directory with a literal; when an evidence question can choose the next passages to read, run that search as `clasify` `source:{tool:"localSearch",query:{...}}` with `predicates:[...]`.
- A typed judgment about one capture file: `clasify` with a `source` reference; save large requests as `.octocode/tmp/octocode-scraping/clasify-request.json`.

```json
{"queries": [{
  "operation": "evaluate",
  "source": {"tool": "localFetch", "query": {
    "path": "/abs/.octocode/octocode-scraping/tmp/<sessionId>/text/<pageId>.clean.part-001.md"
  }},
  "questions": {
    "choice_return": {"type": "noul", "instructions": "Does the page state what a Choice question returns?"},
    "score_return": {"type": "noul", "instructions": "Does the page state what a Score question returns?"}
  }
}]}
```

- Run: `npx -y octocode clasify --input .octocode/tmp/octocode-scraping/clasify-request.json` (MCP `clasify` takes the same JSON when available; inspect the live catalog). A capture over the state limit fails its row: select `ranges`.
- Before citing, follow the emitted captured-source continuation or read the deciding lines with `localFetch`; `hints` are prose, not executable reads. A low answer never proves whole-site absence.
