# Route Selection

Load when choosing how to fetch. Why: pick the cheapest route that can prove the claim without browser or context bloat.

## Default: omit `--provider` — auto-selection handles priority
`scrapingant` (if `SCRAPING_ANT` env set) → `cdp` (if `octocode-chrome-devtools` installed) → `direct`.
Run `provider-check.mjs` (no flags) to confirm which route is active. See `providers.md` for the full matrix.

## Decision tree
1. Local repo/package/code question → use Octocode research/local tools, not web scraping.
2. Public page, any content → omit `--provider`; auto-selection picks the best available. Add `--mode markdown` for LLM-ready text, `--mode extended` for XHR/cookies, `--mode extract` for AI-structured rows (all require scrapingant).
3. JS-rendered page (SPAs, deferred content) → `--browser` flag with scrapingant, or auto-selects cdp if no key. Add `--wait-for <selector>` for readiness proof.
4. Cost-free / no key / plain static HTML → `--provider direct` explicitly, or set no `SCRAPING_ANT` and let auto pick cdp then direct.
5. Live page state, auth, interaction, network evidence, screenshots, or current tab → load `octocode-chrome-devtools` skill and follow read-only CDP scraping (not `--provider cdp` — that is for passive fetch only).
6. CAPTCHA/MFA/private account/high-volume crawl → stop and ask; do not invent bypasses.

## Cost rule
Start with one representative URL and a small output. Expand only after the session corpus has a useful `reports/summary.md` and the user-approved scope requires more.

## Proof rule
Scraping output is a candidate source. Upgrade important claims by reading exact snippets from `text/`, `extracts/`, or `raw/` and citing the original URL metadata in `sources.jsonl`.
