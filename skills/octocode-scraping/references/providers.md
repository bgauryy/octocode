# Providers and ScrapingAnt

Load when choosing `--provider`, checking routes, adding a vendor, or making an approved hosted call. Fetch can vary by vendor; the corpus stays vendor-independent.

## Contract
`fetch({ url, pageId, config, apiKey })` → `FetchResponse` (`scripts/schemas/provider.schema.json`). Corpus and analyzers must not branch on vendor names.

| Provider | Modes | Key | Best for |
|---|---|---|---|
| `direct` | html | no | Static / cheapest proof |
| `cdp` | html | no | Local JS render (sibling chrome-devtools) |
| `scrapingant` | html, markdown, extended, extract | `SCRAPING_ANT` | Hosted anti-bot / markdown / extract: **explicit only** |

Omit `--provider` on html for bounded direct HTTP. Chrome is an evidence-triggered escalation, never an installation-triggered default. Hosted never runs automatically.

## ScrapingAnt (only after the user approves hosted spend)
Default html is keyless, so a hosted call is an explicit, paid choice. Use the neutral scripts; legacy `scrapingant-*.mjs` names are shims.
- Env key `SCRAPING_ANT` resolves through vendored `scripts/octocode-config.mjs` (`propagateOctocodeEnv`). Never print the key.
- `--mode html`: `/v2/general` · `markdown`: `/v2/markdown` · `extended`: `/v2/extended` · `extract`: `/v2/extract`. Usage: `provider-usage.mjs` → `/v2/usage` (sanitized).

```bash
node skills/octocode-scraping/scripts/provider-check.mjs --provider scrapingant
node skills/octocode-scraping/scripts/fetch.mjs --url https://example.com --provider scrapingant --mode html
node skills/octocode-scraping/scripts/fetch.mjs --url https://example.com --provider scrapingant --mode markdown
node skills/octocode-scraping/scripts/fetch.mjs --url https://example.com --provider scrapingant --mode extract --extract-properties "title, content"
node skills/octocode-scraping/scripts/provider-usage.mjs
```

Common options: `--session`, `--out`, `--no-raw`, `--max-raw-bytes`, `--max-text-bytes`, `--extract-links`, `--crawl --max-pages`, `--sitemap`, `--same-domain`, `--delay-ms`, `--browser --wait-for`, `--proxy-type`, `--proxy-country`, `--block-resource`. Full CLI: `fetch.mjs --help`.

## Add a vendor
Add `fetchX` in `scripts/lib/client.mjs`, then register it in `PROVIDERS` (`scripts/lib/providers.mjs`). Human key setup and extension: `docs/PROVIDERS.md`, `docs/ADDING_A_VENDOR.md`.

Next: for the route tree load `references/route-selection.md`; on a hosted `403`/`423` load `references/failure-recovery.md`.
