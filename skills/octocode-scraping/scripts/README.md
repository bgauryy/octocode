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

Schemas: `schemas/graph.schema.json`, `schemas/provider.schema.json`. Libs in `lib/`: `providers.mjs` (registry), `client.mjs` (fetch), `corpus`/`analyzers`/`extractors`/`text` (vendor-agnostic), `args.mjs` (CLI config), `bridge.mjs` (JSON/JSONL readers). `octocode-config.mjs` is a vendored copy of `@octocodeai/config` — `propagateOctocodeEnv` reads `SCRAPING_ANT` from the octocode home; keep it local so the skill runs installed alone (no `package.json`, no npm install).
