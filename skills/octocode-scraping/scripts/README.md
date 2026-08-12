# Scraping Scripts

| Script | Role |
|---|---|
| `fetch.mjs` | Owner: fetch/crawl/extract → `.octocode/tmp/scrape/{sessionId}`; omit `--provider` for keyless html |
| `provider-check.mjs` / `provider-usage.mjs` | Route readiness / hosted credits (no secrets) |
| `scrapingant-*.mjs` | Deprecated shims → `fetch` / `provider-*` |
| `fetch-and-brief.mjs` | Optional fetch + corpus brief |
| `corpus-inspect` / `corpus-find` / `dom-find` / `resource-list` / `graph-navigate` | Query corpus before raw reads (static; live DOM → chrome-devtools) |
| `har-ingest.mjs` | CDP ↔ scrape bridge; `--export-packet` / `--from-cdp-dir` (chrome aliases exist) |
| `corpus-run.mjs` | Local `--regex` / `--script` (chrome alias `corpus-run-local`) |
| `schema-helper.mjs` | Extraction field hints |
| `eval-benchmark-suite.mjs` | Hermetic done gate (`OCTOCODE_LIVE_BENCH=1` for live smoke) |

Schemas: `schemas/graph.schema.json`, `schemas/provider.schema.json`. Libs: `providers.mjs` (registry), `client.mjs` (fetch), `corpus`/`analyzers`/`extractors` (vendor-agnostic).
