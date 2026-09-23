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

## Corpus search pagination

`node scripts/corpus-find.mjs --session-dir <dir> --query <text> --limit 20` returns a ranked page. `--limit` is a positive safe integer (default 20); `--offset` is a non-negative safe integer (default 0). Partial output includes exact counts and `next.page` with an absolute executable `command` and raw `args`. Run that command with those arguments unchanged until `completeness` is `complete` and `next` is `null`. Evidence-file suggestions are separate in `suggestedFiles`. See `../references/session-corpus.md` for the output contract and corpus stability requirement.

Semantic pre-read triage is not a script: call `octocode clasify` directly as in the SKILL.md SCREEN step (each non-empty part an unread `localFetch` resource, ≤ 25 cells per matrix). `corpus-find` ranks whole pages; to locate lines use `corpus-run --regex` (JavaScript regex; `--flags i` for case-insensitive). When clasify is unavailable, fall back to `corpus-find.mjs` lexical triage.

## Focused regressions

Run from the skill directory:

```sh
node --test scripts/tests/corpus-find.test.mjs
node --test scripts/tests/fetch-session.test.mjs
node --test scripts/tests/cdp-client.test.mjs
```

The corpus test executes limit-one continuations over seven fixed matches, checks their exact union, terminal states, and invalid arguments. The CDP test checks the generated default runner through navigation/body extraction and rejects failed stealth setup without starting a real browser. Live browser verification remains separate.
