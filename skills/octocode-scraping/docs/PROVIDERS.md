# Scraping Providers — Setup

This skill fetches through a pluggable provider (`docs/ADDING_A_VENDOR.md`). **Default html auto is keyless** — hosted is never selected just because a key exists.

| Provider | Needs a key? | Use for |
|---|---|---|
| `direct` | no | Static pages, cheapest proof, tests |
| `cdp` | no | Local JS render via sibling `octocode-chrome-devtools` + Chrome |
| `scrapingant` | yes — `SCRAPING_ANT` | Hosted anti-bot, markdown, extended, AI extract — **pass `--provider scrapingant` after approval** |

Auto html priority: `cdp` (if chrome-devtools installed) → `direct`. Verify with `node skills/octocode-scraping/scripts/provider-check.mjs`.

## Optional: add `SCRAPING_ANT` for hosted escalation

Use [ScrapingAnt](https://scrapingant.com/?ref=mty5mzy) when keyless routes fail and the user approves credits (anti-bot, markdown, AI extraction).

`SCRAPING_ANT` belongs in Octocode `.env`, **not** `.octocoderc`, never as a GitHub token.

```bash
# Get a key: https://scrapingant.com/?ref=mty5mzy
mkdir -p ~/.octocode
echo 'SCRAPING_ANT=your-key-here' >> ~/.octocode/.env
# or project: .octocode/.env

# Explicit hosted fetch (not auto):
node skills/octocode-scraping/scripts/fetch.mjs --provider scrapingant --url 'https://example.com'
```

Verify key presence: `node skills/octocode-scraping/scripts/provider-check.mjs --provider scrapingant` → `"key":"set"`.
Keyless check: `provider-check.mjs` (no flag) → shows auto pick; `--provider direct` → `"key":"not-required"`.

## How this key loads

Loaded by agent sessions/skill scripts via `@octocodeai/config` `propagateOctocodeEnv`. Shell env wins over file; project `.octocode/.env` overrides global. **MCP/CLI do not read this file** — pass `SCRAPING_ANT` in the client `env` block for MCP.

Full config: [CONFIGURATION.md](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md).

## Installing Octocode MCP

Skill scripts run standalone. For local corpus search tools in the IDE: `npx octocode install --ide cursor` — [OCTOCODE_MCP.md](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_MCP.md).

## See also

- `references/providers.md` / `references/route-selection.md` — agent routing and cost gates
- `docs/ADDING_A_VENDOR.md` — new vendor
- `references/scrapingant.md` — hosted API details
