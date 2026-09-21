# Script catalog

Use this catalog to select an existing deterministic helper before writing a new scraper or corpus query.

| Script | Role |
|---|---|
| `fetch.mjs` | Fetch, crawl, and extract into `.octocode/tmp/scrape/{sessionId}`; omit `--provider` for keyless HTML |
| `provider-check.mjs` / `provider-usage.mjs` | Route readiness / hosted credits (no secrets) |
| `scrapingant-*.mjs` | Deprecated shims → `fetch` / `provider-*` |
| `fetch-and-brief.mjs` | Optional fetch + corpus brief |
| `corpus-inspect` / `corpus-find` / `dom-find` / `resource-list` / `graph-navigate` | Query corpus before raw reads (static; live DOM → chrome-devtools) |
| `corpus-triage.mjs` | Semantic pre-read gate: every saved part becomes one unread SemanticQuery resource (needs `OCTOCODE_CLASSIFICATION_API`); exact `next.assess` continuations preserve page-local answers; exclusive `read`/`consider`/`skip`, bodies stay out of chat; `--files` accepts only session-contained CDP bodies; `--limit` is matrix size, never a drop limit |
| `har-ingest.mjs` | CDP ↔ scrape bridge; `--export-packet` / `--from-cdp-dir` (chrome aliases exist) |
| `corpus-run.mjs` | Local `--regex` / `--script` (chrome alias `corpus-run-local`) |
| `schema-helper.mjs` | Extraction field hints |

Schemas live in `schemas/graph.schema.json` and `schemas/provider.schema.json`. Libraries under `lib/` own provider registration, fetching, corpus analysis, extraction, argument parsing, and bridge readers. The vendored `octocode-config.mjs` keeps the skill standalone and loads `SCRAPING_ANT` through the standard Octocode environment flow.

## Corpus search pagination

`node scripts/corpus-find.mjs --session-dir <dir> --query <text> --limit 20` returns a ranked page. `--limit` is a positive safe integer (default 20); `--offset` is a non-negative safe integer (default 0). Partial output includes exact counts and `next.page` with an absolute executable `command` and raw `args`. Run that command with those arguments unchanged until `completeness` is `complete` and `next` is `null`. Evidence-file suggestions are separate in `suggestedFiles`. See `../references/session-corpus.md` for the output contract and corpus stability requirement.

`corpus-triage.mjs` discovers all selected file parts and submits successive `semanticAssess` matrices. Its `--limit` must be 1–25 and controls resources per initial query. The runtime captures each resource once per assessment step and exposes ordered page results; the helper follows returned `next.assess` queries unchanged. Output reports `resources`, `matrixBatches`, and assessment calls; no candidate is silently dropped. Stdout returns at most 20 resource errors plus `errorCount` and `errorsTruncated`; the referenced `triage.json` report preserves every raw answer, error, model identity, usage receipt, and explicit routing reduction.

## Focused regressions

Run from the skill directory:

```sh
node --test scripts/tests/corpus-find.test.mjs
node --test scripts/tests/corpus-triage.test.mjs
node --test scripts/tests/fetch-session.test.mjs
node --test scripts/tests/cdp-client.test.mjs
```

The corpus test executes limit-one continuations over seven fixed matches, checks their exact union, terminal states, and invalid arguments. The CDP test checks the generated default runner through navigation/body extraction and rejects failed stealth setup without starting a real browser. Live browser verification remains separate.
