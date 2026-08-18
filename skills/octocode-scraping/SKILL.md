---
name: octocode-scraping
description: "Use when extracting or mapping public web content into a local cited corpus: scrape or crawl a URL/docs site, pull tables/pricing/product fields, diagnose blocked or thin pages, or answer from saved pages. Phrases like scrape this URL, crawl the docs, build a corpus, extract pricing. Prefer keyless fetch; ask before hosted spend. Live clicks/HAR/perf → octocode-chrome-devtools."
---

# Octocode Scraping

Flow: `FRAME → POLICY → ROUTE → FETCH → CORPUS → SEARCH → CITE → RECOVER`.

FRAME before the first fetch: fix target URL/domain, goal, depth, and output shape — vague ask → `references/user-inputs.md`.

Defaults: one public URL, `--mode html`, omit `--provider` (keyless `cdp`→`direct`), session `.octocode/tmp/scrape/{sessionId}`, compact stdout. Search corpus before refetch. Live interaction → chrome-devtools on one port, then `har-ingest` + `corpus-run` into the same session. Ask before auth, hosted spend, crawl widen, CAPTCHA/MFA, destructive actions. Cite paths + URL metadata — not raw dumps.

Stop when: two same-class failures (report evidence, route tried, sanitized status, next approval); hosted `403` (wrong key or credits gone — status only, no retry); CAPTCHA/MFA, auth wall, or cookie/profile transfer needed; still blocked after one `cdp` try (ask before `--provider scrapingant`); personal data, form submits, purchases, sends, deletes, or account changes in scope; the saved corpus already proves the claim (cite it, do not refetch); crawl widen before `reports/summary.md` is useful. Recovery table: `references/failure-recovery.md`.

## Route (pick one)
| Need | Do | Skip |
|---|---|---|
| Vague scrape | `--mode html`, omit `--provider` | markdown / auto hosted |
| See auto pick | `scripts/provider-check.mjs` | guessing |
| Fetch/crawl | `scripts/fetch.mjs` | deprecated `scrapingant-fetch` |
| Corpus on disk | `corpus-inspect` → find helpers → `corpus-run` | blind refetch |
| Live click/DOM/auth | chrome-devtools | `--provider cdp` alone for interaction |
| Page health | chrome measure + `measure-query` | hosted scrape for scores |
| CDP → corpus | `scripts/har-ingest.mjs --from-cdp-dir` | new sessionId |
| Prove field | `scripts/corpus-run.mjs --regex\|--script` | reopen browser |
| Still blocked | evidence + ask → `--provider scrapingant` | silent spend |

## Scripts (Node, no install; every one takes `--help`)
| When | Run |
|---|---|
| fetch / crawl / extract a URL — the owner of every network call | `scripts/fetch.mjs --url <u> [--mode html] [--crawl --same-domain --max-pages <n>] [--no-raw]` |
| want the fetch plus an immediate corpus brief in one shot | `scripts/fetch-and-brief.mjs --url <u>` (wraps `fetch.mjs` → `corpus-inspect.mjs`) |
| before routing or hosted spend: which provider auto-wins, credits left | `scripts/provider-check.mjs [--provider <p>]`, `scripts/provider-usage.mjs` — sanitized, never prints the key |
| read a saved session first, then search it | `scripts/corpus-inspect.mjs --session-dir <d> [--page <n>]`, then `scripts/corpus-find.mjs --session-dir <d> --query <t>` |
| pull static DOM, assets, or graph paths out of the corpus (live DOM → chrome-devtools) | `scripts/dom-find.mjs --kind form/button/table`, `scripts/resource-list.mjs --kind asset/external`, `scripts/graph-navigate.mjs --from <nodeId>` — each with `--session-dir <d>` |
| prove one field locally instead of reopening a browser | `scripts/corpus-run.mjs --session-dir <d> --roots cdp,extracts --regex <re>` (or `--script <file>`, `--concat-parts`) |
| bridge chrome-devtools artifacts into this session, or hand a packet back | `scripts/har-ingest.mjs --session-dir <d> --from-cdp-dir <run>`; reverse with `--export-packet` |
| need field names before an extraction | `scripts/schema-helper.mjs --intent "extract pricing and features"` |
| an old transcript uses the legacy names | deprecated shims `scripts/scrapingant-fetch.mjs`, `scripts/scrapingant-check.mjs`, `scripts/scrapingant-usage.mjs` forward to `fetch.mjs` / `provider-check.mjs` / `provider-usage.mjs` — call the new names |
| editing or extending a script | shared modules in `scripts/lib/` (`providers` registry, `client` fetch, `corpus`, `analyzers`, `extractors`, `text`, `args`, `bridge`); env/key resolution vendored in `scripts/octocode-config.mjs`; JSON contracts in `scripts/schemas/` |

Full flags, roles, and the library map: read `scripts/README.md` before adding a flag or a vendor.

## References
| When | Load |
|---|---|
| frame a vague scope | `references/user-inputs.md` |
| legal/safety/privacy policy | `references/scraping-policy.md` |
| cost / keyless vs hosted route | `references/route-selection.md` |
| provider registry / add a vendor | `references/providers.md` |
| hosted API, after approval | `references/scrapingant.md` |
| search a saved session corpus | `references/session-corpus.md` |
| site graph / navigation / workflows | `references/website-analysis.md` |
| stdout and corpus file shapes | `references/data-contract.md` |
| extract quality and citations | `references/extraction-quality.md` |
| live browser bridge playbook | `references/browser-scraping.md` |
| blocked, thin, or oversized results | `references/failure-recovery.md` |
