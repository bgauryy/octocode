# ScrapingAnt

Load when hosted spend is already user-approved — never before. Why: default html is keyless, so hosted calls must be an explicit, paid choice. Prefer neutral scripts: `fetch.mjs --provider scrapingant`, `provider-check.mjs --provider scrapingant`, `provider-usage.mjs`. Legacy `scrapingant-*.mjs` names are shims.

## Auth and endpoints
- Env key: `SCRAPING_ANT`; resolved through vendored `scripts/octocode-config.mjs` (`propagateOctocodeEnv`). Never print the key.
- `--mode html`: `/v2/general` · `markdown`: `/v2/markdown` · `extended`: `/v2/extended` · `extract`: `/v2/extract`.
- Usage: `provider-usage.mjs` → `/v2/usage` (sanitized).

## Examples
```bash
node skills/octocode-scraping/scripts/provider-check.mjs --provider scrapingant
node skills/octocode-scraping/scripts/fetch.mjs --url https://example.com --provider scrapingant --mode html
node skills/octocode-scraping/scripts/fetch.mjs --url https://example.com --provider scrapingant --mode markdown
node skills/octocode-scraping/scripts/fetch.mjs --url https://example.com --provider scrapingant --mode extract --extract-properties "title, content"
node skills/octocode-scraping/scripts/provider-usage.mjs
```

## Common options
`--session`, `--out`, `--no-raw`, `--max-raw-bytes`, `--max-text-bytes`, `--extract-links`, `--crawl --max-pages`, `--sitemap`, `--same-domain`, `--delay-ms`, `--browser --wait-for`, `--proxy-type`, `--proxy-country`, `--block-resource`.

Full CLI: `fetch.mjs --help`. Human key setup: `docs/PROVIDERS.md`.

Next: for the keyless alternative load `references/route-selection.md`; for the vendor registry load `references/providers.md`; on a hosted `403`/`423` load `references/failure-recovery.md`.
