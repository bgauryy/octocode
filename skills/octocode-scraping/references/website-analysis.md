# Website Analysis

Load when the user wants to understand a site, find smart links, map workflows, or analyze scraped data. Why: agents should navigate a local corpus instead of rereading raw pages.

## Data model
- `AGENT_INDEX.json`: first-read contract, warnings, totals, search targets, pagination hints.
- `graph/graph.json`: unified automation graph — pages, links, forms, buttons, tables, resources, pagination, typed edges, risks, confidence, and source evidence; validated by `schemas/graph.schema.json`. Prefer this for downstream bots/tools that need one portable file.
- `indexes/pages-001.json`: paginated *corpus* rows for large crawls (this skill's own output pagination — not the target site's). Read next page files only as needed.
- `graph/site-graph.json`: pages, internal/subdomain edges, heading outlines, and top link candidates — richer detail behind the unified graph's nodes.
- `indexes/top-links.jsonl`: ranked links by same-host/domain, label quality, content signals, and shallow depth. A link's `workflowType: "pagination"` means the *target site* has more pages of this content — detected structurally from `rel="next"/"prev"` or a `pagination`/`pager` class, never from link text alone (icon-only "next" arrows have no text to match).
- `extracts/resources.jsonl`: non-navigational assets — `script`, `stylesheet`, `image`, `media`, `feed` — read directly off tag attributes (`src`/`href`), not classified. Useful for third-party/tracking-script inventory or asset discovery; these never carry a `workflowType`.

## Smart analysis workflow
1. Read `AGENT_INDEX.json`; stop on warnings unless partial evidence is acceptable.
2. Read `indexes/pages-001.json` for the page list; paginate only when the target is not on the first page.
3. Read `graph/graph.json` for automation nodes/edges; use `graph/site-graph.json` for richer page/link detail.
4. Search `text/*.clean.part-*.md` and `extracts/` only after choosing candidate pages/links.
5. Cite exact text chunks plus `sources.jsonl`; use raw only to audit extraction.

## Workflow graph best practices
- Treat links/actions as candidates, not proof; prefer visible labels, same-host links, and nodes with source evidence.
- Score task paths: homepage → docs/feature/pricing/API/contact/examples/changelog/pagination; de-rank skip links, hash-only nav, and generic menus.
- Preserve blocked/error pages in warnings; do not hide them as empty data.
- For “understand all website”, crawl a bounded allowlist first, inspect graph quality, then expand with approval.
- For “get every page of this listing/archive”, follow `paginates_to` / `workflowType:"pagination"` edges rather than guessing URL patterns — it's the site's own declared next/prev structure.

Next: query the graph from disk with `scripts/graph-navigate.mjs --session-dir <d>`; for the file field contracts load `references/data-contract.md`; for live actionability load `references/browser-scraping.md`.
