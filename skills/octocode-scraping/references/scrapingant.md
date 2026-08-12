# ScrapingAnt

Load **only after** user-approved hosted spend. Default html is keyless (`route-selection.md`). Prefer neutral scripts: `fetch.mjs --provider scrapingant`, `provider-check.mjs --provider scrapingant`, `provider-usage.mjs`. Legacy `scrapingant-*.mjs` names are shims.

## Auth and endpoints
- Env key: `SCRAPING_ANT`; load through `@octocodeai/config`. Never print the key.
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

Full CLI: `fetch.mjs --help`. Registry: `providers.md`. Human key setup: `docs/PROVIDERS.md`.
