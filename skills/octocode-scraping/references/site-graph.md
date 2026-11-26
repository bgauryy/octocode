Load when mapping a site, choosing the next pages, following a listing, locating DOM elements, or extracting rows with an extractor.

# Site graph and extraction

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
