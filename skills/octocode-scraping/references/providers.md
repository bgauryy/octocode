Load when the route or `--provider` is unclear, or before a broad crawl or hosted call.

## Scope

- Ask only for a missing choice that changes the work. Vague request: SKILL.md defaults, no auth; return the session path and next search targets.
- Map a site: `fetch.mjs --url <u> --crawl --max-pages 20`.
- Cheapest route that proves the claim, one per need.

## Route

| Need | Route | Key |
|---|---|---|
| Public static page | `direct`, `--mode html` | no |
| Thin 200 or JS shell | `octocode-chrome-devtools` capture, bridged into the corpus | no |
| Local JS render, no clicks | `cdp`, `--mode html` | no |
| Auth, clicks, network, screenshots | `octocode-chrome-devtools` ([browser-scraping](browser-scraping.md)) | no |
| Hosted anti-bot, `markdown`, `extended`, `extract` | `scrapingant`, explicit choice | `SCRAPING_ANT` |

- Default `direct`; installing chrome-devtools or setting `SCRAPING_ANT` never auto-selects a provider. Only a non-html `--mode` implies `scrapingant` (fails early without the key).

## ScrapingAnt

- No `scrapingant` call until the user approves hosted spend.
- `provider-usage.mjs --check` reports only `"key":"set"` (exit 1 `missingKey` when unset), offline; without `--check` it reads `/v2/usage` (sanitized plan and credits). Modes map to `/v2/general` (html), `/v2/markdown`, `/v2/extended`, `/v2/extract`.

```bash
node <skill-folder>/scripts/provider-usage.mjs --check
node <skill-folder>/scripts/fetch.mjs --url https://example.com --provider scrapingant --mode html
node <skill-folder>/scripts/fetch.mjs --url https://example.com --provider scrapingant --mode extract --extract-properties "title, content"
node <skill-folder>/scripts/provider-usage.mjs
```

- These scripts load Octocode home and trusted workspace `.env` values through the bundled shared config loader; process environment values take precedence. To save the key, use `npx -y octocode config set SCRAPING_ANT --stdin`.
- Options: `fetch.mjs --help`; hosted: `--browser --wait-for`, `--proxy-type`, `--proxy-country`, `--block-resource`.
- Hosted `403`/`423`: stop ([recovery](recovery.md)).

## Key setup (humans)

- Key: [ScrapingAnt dashboard](https://app.scrapingant.com/); API: [docs.scrapingant.com](https://docs.scrapingant.com/).
- Store and pass `SCRAPING_ANT` using [shared configuration handling](configuration.md); the commands above show this provider's consumers.
