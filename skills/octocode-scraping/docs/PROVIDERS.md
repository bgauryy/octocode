# Provider setup

Default HTML routing is keyless; a configured hosted key never auto-selects paid scraping.

| Provider | Key | Use |
|---|---|---|
| `direct` | none | static pages and tests |
| `cdp` | none | local JS render through `octocode-chrome-devtools` |
| `scrapingant` | `SCRAPING_ANT` | approved hosted anti-bot, markdown, extended, or extract |

Automatic HTML routing is `direct`. The installed skill set never changes that choice. Escalate to `cdp` only when the direct corpus returns a browser recommendation or proves that rendered DOM, interaction, or live network evidence is required. Inspect it with `node scripts/provider-check.mjs` from the skill folder.

## Optional hosted setup

After paid escalation is authorized, set `SCRAPING_ANT` in the shell, the trusted project `.octocode/.env`, or `<HOME>/.octocode/.env`. Do not put the value in logs or chat.

```bash
node skills/octocode-scraping/scripts/provider-check.mjs --provider scrapingant
node skills/octocode-scraping/scripts/fetch.mjs --provider scrapingant --url 'https://example.com'
```

The check reports only `"key":"set"`; `provider-usage.mjs` returns sanitized plan/credit status. Skill scripts read `SCRAPING_ANT` from their process environment. If the value is stored in `<HOME>/.octocode/.env`, pass it through the host or capture `npx octocode config get SCRAPING_ANT` into the script environment without displaying it.

Skill scripts run standalone. To add another vendor, follow [ADDING_A_VENDOR.md](ADDING_A_VENDOR.md). Agent cost and routing rules live in `references/providers.md` and `references/route-selection.md`.
