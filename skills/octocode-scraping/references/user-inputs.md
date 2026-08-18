# User Inputs

Load before broad crawl, extract schemas, or workflow analysis. Why: better inputs → smaller corpora.

Ask: goal · scope (one URL / list / same-domain max-pages) · output shape · evidence strictness · boundaries (auth, personal data, forms, CAPTCHA, rate limits).

## Vague default
One public URL, `--mode html`, omit `--provider`, no auth, no broad crawl, session `.octocode/tmp/scrape/{sessionId}` — return session path + next search targets. Not markdown / not ScrapingAnt by default.

| Intent | Route |
|---|---|
| fetch/scrape page | `--mode html` (omit `--provider`) |
| pretty markdown | ask — needs scrapingant + key |
| JS / thin after direct | `--provider cdp` or chrome-devtools |
| structured fields | ask — `--mode extract` (hosted) |
| site/workflows | bounded `--crawl --same-domain --max-pages` |
| live click / HAR / perf | chrome-devtools (`browser-scraping.md`) |

Next: for the route tree load `references/route-selection.md`; for the vendor registry load `references/providers.md`; when boundaries or legality are unclear load `references/scraping-policy.md`.
