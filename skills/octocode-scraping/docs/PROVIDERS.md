# Provider setup

Default HTML routing is keyless; a configured hosted key never auto-selects paid scraping.

| Provider | Key | Use |
|---|---|---|
| `direct` | none | static pages and tests |
| `cdp` | none | local JS render through `octocode-chrome-devtools` |
| `scrapingant` | `SCRAPING_ANT` | approved hosted anti-bot, markdown, extended, or extract |

Automatic order is `cdp` when available, then `direct`. Inspect it with `node scripts/provider-check.mjs` from the skill folder.

## Optional hosted setup

After you approve paid escalation, obtain a [ScrapingAnt key](https://scrapingant.com/?ref=mty5mzy) and set `SCRAPING_ANT` in the shell, project `.octocode/.env`, or global Octocode `.env`. Do not put it in `.octocoderc`, a GitHub token field, logs, or chat.

```bash
node skills/octocode-scraping/scripts/provider-check.mjs --provider scrapingant
node skills/octocode-scraping/scripts/fetch.mjs --provider scrapingant --url 'https://example.com'
```

The check reports only `"key":"set"`; `provider-usage.mjs` returns sanitized plan/credit status. Shell values override files; project `.octocode/.env` overrides global. Skill scripts load these through the vendored config module. MCP/CLI processes need `SCRAPING_ANT` in their own client environment.

Skill scripts run standalone. To add another vendor, follow [ADDING_A_VENDOR.md](ADDING_A_VENDOR.md). Agent cost and routing rules live in `references/providers.md`, `references/route-selection.md`, and `references/scrapingant.md`.
