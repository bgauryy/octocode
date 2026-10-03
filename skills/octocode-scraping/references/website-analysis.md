# Website Analysis

Load when you want to understand a site, find smart links, map workflows, or analyze scraped data. Why: agents must navigate a local corpus instead of rereading raw pages.

## Data model
- `AGENT_INDEX.json`: first-read contract, warnings, totals, search targets, pagination hints.
- `graph/graph.json`: unified automation graph — pages, links, forms, buttons, tables, resources, pagination, typed edges, risks, confidence, and source evidence; validated by `schemas/graph.schema.json`. Prefer this for downstream bots/tools that need one portable file.
- `indexes/pages-001.json`, `pages-002.json`, …: paginated *corpus* rows for large crawls (this skill's own output pagination — not the target site's).
- `graph/site-graph.json`: pages, internal/subdomain edges, heading outlines, and top link candidates — richer detail behind the unified graph's nodes.
- `indexes/top-links.jsonl`: ranked links by same-host/domain, label quality, content signals, and shallow depth. A link's `workflowType: "pagination"` means the *target site* has more pages of this content — detected structurally from `rel="next"/"prev"` or a `pagination`/`pager` class, never from link text alone (icon-only "next" arrows have no text to match).
- `extracts/resources.jsonl`: non-navigational assets — `script`, `stylesheet`, `image`, `media`, `feed` — read directly off tag attributes (`src`/`href`), not classified. Useful for third-party/tracking-script inventory or asset discovery; these never carry a `workflowType`.

## Workflow graph best practices
- Treat links/actions as candidates, not proof; prefer visible labels, same-host links, and nodes with source evidence.
- Score task paths: homepage → docs/feature/pricing/API/contact/examples/changelog/pagination; de-rank skip links, hash-only nav, and generic menus.
- For “understand all website”, inspect graph quality of the bounded crawl before asking to expand.
- For “get every page of this listing/archive”, follow `paginates_to` / `workflowType:"pagination"` edges rather than guessing URL patterns — it's the site's own declared next/prev structure.

Next: query the graph from disk with `scripts/graph-navigate.mjs --session-dir <d>`; for the file field contracts load `references/data-contract.md`; for live actionability load `references/browser-scraping.md`.
