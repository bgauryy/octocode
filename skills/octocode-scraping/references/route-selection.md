# Route Selection

Load when choosing how to fetch. Why: cheapest route that can prove the claim. Registry: `providers.md`.

## Default
Omit `--provider` on html → `cdp` (if chrome-devtools present) → `direct`. `SCRAPING_ANT` does not auto-select. Check: `provider-check.mjs`.

## Tree
1. Repo/code question → Octocode research, not scrape.
2. Public static → omit `--provider` or `direct`; prove from corpus.
3. Thin/JS after direct → `cdp` or chrome-devtools live proof.
4. Auth / clicks / network / screenshots → chrome-devtools (one port).
5. Still blocked → stop + evidence; ask before `--provider scrapingant`.
6. `markdown` / `extended` / `extract` → scrapingant + key; ask if new spend.
7. CAPTCHA/MFA / private / high-volume → stop and ask.

One URL first. Expand crawl only after `reports/summary.md` is useful. Cite `text/`/`extracts/`/`cdp/` + `sources.jsonl`.
