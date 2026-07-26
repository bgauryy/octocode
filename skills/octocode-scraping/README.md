# Octocode Scraping Skill

Responsible web scraping and extraction workflow for Octocode agents.

## scrape → corpus → proof

This is **not a basic API wrapper**. The strongest differentiator is the Octocode proof loop:

```text
fetch/crawl/extract
→ normalized .octocode/tmp/scrape/{sessionId}/ corpus + AGENT_INDEX.json
→ Octocode local tools search/read/prove over agent index, clean chunks, and extracts
→ exact source citations without context bloat
```

Scraped pages become a local mini-corpus agents can inspect with `localViewStructure`, `localSearchCode`, and `localGetFileContent` instead of dumping raw HTML/API payloads into chat.

## What this skill separates

- Raw provider output: saved under `raw/` for audit/debug only.
- Cleaned agent-facing text: chunked under `text/*.clean.part-*.md`.
- Structured extracts: metadata, headings, links, resources (JS, CSS, images, media, feeds), extended response rows, AI extraction output, costs.
- Reports: compact summaries, failures, crawl summary, costs.
- Source metadata: `sources.jsonl` records URL/status/content-type/fetch timing.
- URL-to-file map: `MAP.md` and `page-map.json` show what URL produced which files.

Safety posture: vendor keys are loaded from Octocode env, never printed, secret-like passthrough params are rejected, and raw content stays out of chat by default. This is designed for Octocode dogfooding: scrape with APIs, then prove with Octocode local tools.

## Vendor-pluggable fetching

Fetching goes through a provider abstraction (`scripts/lib/providers.mjs`) — direct HTTP, browser-backed CDP, or a hosted anti-bot/extraction provider. Corpus building, extraction, and graph analysis never depend on which route fetched the page. See `docs/PROVIDERS.md` (setup) and `docs/ADDING_A_VENDOR.md` (adding a vendor).

## Modes and features

```bash
# Check env (add --provider direct to check a keyless provider)
node skills/octocode-scraping/scripts/provider-check.mjs

# Basic page fetches
node skills/octocode-scraping/scripts/fetch.mjs --url https://example.com --mode html
node skills/octocode-scraping/scripts/fetch.mjs --url https://example.com --mode markdown

# No vendor, no key — plain HTTP
node skills/octocode-scraping/scripts/fetch.mjs --url https://example.com --mode html --provider direct

# ScrapingAnt extended endpoint: headers/XHRs/iframes/cookies redacted
node skills/octocode-scraping/scripts/fetch.mjs --url https://example.com --mode extended

# ScrapingAnt AI extraction endpoint
node skills/octocode-scraping/scripts/fetch.mjs --url https://example.com --mode extract --extract-properties "title, content"

# Small allowlisted crawl
node skills/octocode-scraping/scripts/fetch.mjs --url https://docs.scrapingant.com/api-basics --mode markdown --crawl --same-domain --max-pages 2 --delay-ms 1000

# Sanitized usage/plan lookup
node skills/octocode-scraping/scripts/provider-usage.mjs

# Deterministic eval suite
node skills/octocode-scraping/scripts/eval-scraping.mjs
```

## Session corpus

Each fetch prints compact JSON and writes:

```text
.octocode/tmp/scrape/{sessionId}/
  AGENT_INDEX.json          # compact machine index; read first
  manifest.json
  MAP.md                    # human URL → file map
  page-map.json             # machine URL → file map
  graph/graph.json          # automation graph: pages, data, actions, risks, evidence
  graph/site-graph.json     # smart link/workflow graph (detail behind graph.json's nodes)
  graph/workflows.json      # scored workflow candidates
  schemas/graph.schema.json # copied in every session — validate graph.json without this skill installed
  indexes/pages-001.json    # paginated page rows
  indexes/top-links.jsonl   # ranked link candidates
  README.md
  sources.jsonl             # URL/status/content-type/fetch metadata + Ant-credits-cost when available
  pages/page-001.json
  raw/page-001.html|json    # audit/debug only
  text/page-001.md          # compact text index
  text/page-001.clean.part-001.md
  extracts/metadata.json
  extracts/page-001-metadata.json
  extracts/headings.jsonl
  extracts/links.jsonl
  extracts/resources.jsonl  # JS, CSS, images, media, feeds, structural (no workflowType)
  extracts/costs.jsonl
  reports/summary.md
  reports/crawl-summary.md
  reports/costs.md
  reports/failures.md
```

## Agent search flow

1. `localViewStructure` on the session folder.
2. Read `AGENT_INDEX.json` first, then `indexes/pages-001.json`, `graph/graph.json`, and `graph/site-graph.json` for pages, actions, links, pagination, and workflow paths.
3. Search `text/*.clean.part-*.md`, `extracts/`, `indexes/`, `graph/`, `snippets/`, and `reports/`.
4. Use `localGetFileContent` for exact evidence ranges.
5. Treat `warnings` / `targetLikelyError` as partial-or-failed evidence even when provider status is 200.
6. Read `raw/` only when extraction is disputed or debugging provider output.

This keeps context small while preserving raw auditability and source-citable proof.

## Docs

- `docs/PROVIDERS.md` — configure `SCRAPING_ANT` in `~/.octocode/.env`, use the keyless `direct` provider, install the Octocode MCP server.
- `docs/ADDING_A_VENDOR.md` — the vendor contract and how to add a new one.
- `references/` — terse, agent-facing routing docs (policy, route selection, data contract, failure recovery, providers, brainstorm roadmap).
