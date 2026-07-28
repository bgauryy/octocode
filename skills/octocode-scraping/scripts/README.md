# Scraping Scripts

- `provider-check.mjs`: neutral alias for provider readiness without printing secrets; `--provider direct` needs none.
- `fetch.mjs`: neutral alias to fetch/crawl/extract public pages into `.octocode/tmp/scrape/{sessionId}`; `--provider scrapingant|direct|cdp`.
- `provider-usage.mjs`: neutral alias for hosted provider usage/credit lookup.
- `scrapingant-check.mjs`, `scrapingant-fetch.mjs`, `scrapingant-usage.mjs`: compatibility entrypoints kept for existing commands.
- `fetch-and-brief.mjs`: run fetch and return a compact corpus brief + next commands.
- `corpus-inspect.mjs`: summarize a saved corpus by pages, workflows, and top links.
- `corpus-find.mjs`: find pages/workflows/links/elements/resources in a saved corpus before raw search.
- `dom-find.mjs`: filter extracted DOM/element rows (static) by kind, workflow, page, and query. For live actionability checks (visible/disabled/covered/accessible-name) on a real page, delegate to `octocode-chrome-devtools`'s `examples/dom-operations-check.mjs` instead — this skill never drives a live browser.
- `resource-list.mjs`: filter/facet navigation links by kind (incl. `pagination`), workflow, host, score, and query. Asset resources (JS, CSS, images, feeds) are in `extracts/resources.jsonl` — search via `corpus-find.mjs`.
- `graph-navigate.mjs`: return graph routes and workflow candidates for smart navigation.
- `schema-helper.mjs`: deterministic extraction schema suggestions from user intent.
- `eval-scraping.mjs`: deterministic behavior checks.
- `eval-large-crawl.mjs`: 30-page mock crawl stays paginated and `AGENT_INDEX.json` stays small.
- `eval-unified-graph.mjs`: `graph/graph.json` conforms to `schemas/graph.schema.json` and cross-page edges resolve correctly.
- `schemas/graph.schema.json`: JSON Schema for the unified automation graph; copied into every session at `schemas/graph.schema.json`.
- `schemas/provider.schema.json`: JSON Schema for the vendor `FetchResponse`/`ProviderDescriptor` contract; see `lib/providers.mjs`.
- `eval-providers.mjs`: registry conforms to the provider schema; a real keyless `direct` fetch and a mocked one match `scrapingant`'s corpus shape.
- `eval-website-analysis.mjs`: graph/index/element/workflow analysis checks.
- `eval-agent-helpers.mjs`: helper scripts work from local corpus data.
- `eval-smart-navigation.mjs`: DOM/resource/graph filter helpers work from local data.

Helpers in `lib/` own parsing, provider calls, corpus writing, extraction, text cleanup, and graph/index analysis. `lib/providers.mjs` is the only file that knows which vendors exist; `lib/client.mjs` implements each vendor's `fetch`; `lib/corpus.mjs`/`analyzers.mjs`/`extractors.mjs` consume only the generic `FetchResponse` shape.
