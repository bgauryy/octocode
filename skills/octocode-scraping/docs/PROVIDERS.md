# Scraping Providers

Default HTML routing is keyless; a configured hosted key never auto-selects paid scraping.

| Provider | Key | Use |
|---|---|---|
| `direct` | none | static pages and tests |
| `cdp` | none | local JS render through `octocode-chrome-devtools` |
| `scrapingant` | `SCRAPING_ANT` | approved hosted anti-bot, markdown, extended, or extract |

Auto order is `cdp` when available, then `direct`. Inspect it with `node skills/octocode-scraping/scripts/provider-check.mjs`.

## Optional hosted setup

After the user approves paid escalation, obtain a [ScrapingAnt key](https://scrapingant.com/?ref=mty5mzy) and set `SCRAPING_ANT` in the shell, project `.octocode/.env`, or global Octocode `.env`. Do not put it in `.octocoderc`, a GitHub token field, logs, or chat.

```bash
node skills/octocode-scraping/scripts/provider-check.mjs --provider scrapingant
node skills/octocode-scraping/scripts/fetch.mjs --provider scrapingant --url 'https://example.com'
```

The check reports only `"key":"set"`; `provider-usage.mjs` returns sanitized plan/credit status. Shell values override files; project `.octocode/.env` overrides global. Skill scripts load these through the vendored config module. MCP/CLI processes need `SCRAPING_ANT` in their own client environment.

Skill scripts run standalone. Optional IDE search tools install with `npx octocode install --ide cursor`. To add another vendor, follow `docs/ADDING_A_VENDOR.md`; agent cost/routing rules live in `references/providers.md`, `references/route-selection.md`, and `references/scrapingant.md`.
