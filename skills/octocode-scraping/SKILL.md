---
name: octocode-scraping
description: "Use when extracting or mapping public web content into a local cited corpus: scrape or crawl a URL/docs site, pull tables/pricing/product fields, diagnose blocked or thin pages, or answer from saved pages. Phrases like scrape this URL, crawl the docs, build a corpus, extract pricing. Prefer keyless fetch; ask before hosted spend. Live clicks/HAR/perf → octocode-chrome-devtools."
---

# Octocode Scraping

Flow: `FRAME → POLICY → ROUTE → FETCH → CORPUS → SEARCH → CITE → RECOVER`.

Defaults: one public URL, `--mode html`, omit `--provider` (keyless `cdp`→`direct`), session `.octocode/tmp/scrape/{sessionId}`, compact stdout. Search corpus before refetch. Live interaction → chrome-devtools on one port, then `har-ingest` + `corpus-run` into the same session. Ask before auth, hosted spend, crawl widen, CAPTCHA/MFA, destructive actions. Cite paths + URL metadata — not raw dumps.

## Route (pick one)
| Need | Do | Skip |
|---|---|---|
| Vague scrape | `--mode html`, omit `--provider` | markdown / auto hosted |
| See auto pick | `provider-check.mjs` | guessing |
| Fetch/crawl | `fetch.mjs` | deprecated `scrapingant-fetch` |
| Corpus on disk | `corpus-inspect` → find helpers → `corpus-run` | blind refetch |
| Live click/DOM/auth | chrome-devtools | `--provider cdp` alone for interaction |
| Page health | chrome measure + `measure-query` | hosted scrape for scores |
| CDP → corpus | `har-ingest --from-cdp-dir` | new sessionId |
| Prove field | `corpus-run --regex\|--script` | reopen browser |
| Still blocked | evidence + ask → `--provider scrapingant` | silent spend |

## References
| When | Load |
|---|---|
| vague scope | `references/user-inputs.md` |
| legal/safety | `references/scraping-policy.md` |
| cost / keyless vs hosted | `references/route-selection.md` |
| registry / vendors | `references/providers.md` |
| hosted API (after approval) | `references/scrapingant.md` |
| search saved session | `references/session-corpus.md` |
| graph / nav | `references/website-analysis.md` |
| stdout shapes | `references/data-contract.md` |
| extract quality | `references/extraction-quality.md` |
| live bridge | `references/browser-scraping.md` |
| blocked / thin | `references/failure-recovery.md` |

Scripts map: `scripts/README.md`. Skill smoke: `scripts/eval-scraping-skill.mjs` (`--self-test` / `--triggers`). Done: `eval-benchmark-suite.mjs` after edits; `OCTOCODE_LIVE_BENCH=1` only for fetch/network changes.
