# Script catalog

Use this catalog to select an existing deterministic helper before writing a new scraper or corpus query.

| Script | Role |
|---|---|
| `fetch.mjs` | Fetch, crawl, and extract into `.octocode/tmp/scrape/{sessionId}`; omit `--provider` for keyless HTML |
| `provider-check.mjs` / `provider-usage.mjs` | Route readiness / hosted credits (no secrets) |
| `scrapingant-*.mjs` | Deprecated shims → `fetch` / `provider-*` |
| `fetch-and-brief.mjs` | Optional fetch + corpus brief |
| `corpus-inspect` / `corpus-find` / `dom-find` / `resource-list` / `graph-navigate` | Query corpus before raw reads (static; live DOM → chrome-devtools) |
| `har-ingest.mjs` | CDP ↔ scrape bridge; `--export-packet` / `--from-cdp-dir` (chrome aliases exist) |
| `corpus-run.mjs` | Local `--regex` / `--script` (chrome alias `corpus-run-local`) |
| `schema-helper.mjs` | Extraction field hints |

Schemas live in `schemas/graph.schema.json` and `schemas/provider.schema.json`. Libraries under `lib/` own provider registration, fetching, corpus analysis, extraction, argument parsing, and bridge readers. The vendored `octocode-config.mjs` keeps the skill standalone and loads `SCRAPING_ANT` through the standard Octocode environment flow.
