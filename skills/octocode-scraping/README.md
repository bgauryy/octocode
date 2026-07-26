# Octocode Scraping Skill

Scrape public pages, turn them into a local searchable corpus, then use Chrome/CDP to validate live actions when static HTML is not enough.

## Quick setup

1. **Install the Octocode MCP server** — full guide: https://octocode.ai/installation
   - Run `npx octocode install` or target one client with `npx octocode install --ide cursor`.
   - Or add manually to your MCP config:
     ```json
     {
       "octocode": {
         "command": "npx",
         "type": "stdio",
         "args": ["octocode-mcp@latest"]
       }
     }
     ```

2. **Install both local skills** — static scrape/corpus plus browser automation/CDP:
   - `npx octocode skill --name octocode-scraping`
   - `npx octocode skill --name octocode-chrome-devtools`
   - `npx octocode skill check --json`

3. **Optional but recommended: add ScrapingAnt** for better real-site results. See [Recommended provider](#recommended-provider-scrapingant).

## Start with this prompt

Copy this into your agent and replace the placeholders:

> Use Octocode scraping on `<URL>`. Check all links, resources, forms, buttons, pagination, search inputs, and likely workflows. Build the scrape corpus and graph, then use Chrome DevTools/CDP to validate live actionability. Tell me how to make `<GOAL>` work, cite the saved artifacts, and if anything is blocked or JS-only, diagnose whether it is blocked, JS-shell, selector mismatch, consent/region, or timing/hydration.

Short version:

> Use octocode scrape and check all links/actions on `<URL>`, then automate/validate in Chrome and explain how to make `<GOAL>`.

## Recommended provider: ScrapingAnt

Get better scraping results with ScrapingAnt: [https://scrapingant.com](https://scrapingant.com/?ref=mty5mzy)

[![ScrapingAnt — Web Scraping API, Proxies, and AI Extraction](https://scrapingant.com/images/scrapingant.png)](https://scrapingant.com/?ref=mty5mzy)

The skill works without a key: default/keyless routes are Chrome browser/CDP for live pages and curl-like direct HTTP for simple static pages. For tougher real sites, add a [`SCRAPING_ANT`](docs/PROVIDERS.md) key to unlock hosted browser rendering, rotating proxies, markdown conversion, and AI extraction.

Public ScrapingAnt page evidence describes the service as Headless Chrome + 3M+ rotating proxies + AI extraction behind one API, with a hosted MCP server for AI agents and 10K free credits/no card.

Add your key with: `mkdir -p ~/.octocode && printf 'SCRAPING_ANT=%s\n' 'your-key-here' >> ~/.octocode/.env`

Full setup and fallback routing: [`docs/PROVIDERS.md`](docs/PROVIDERS.md).

## How the workflow works

1. **Static first** — fetch/crawl/extract public pages into `.octocode/tmp/scrape/<session>/`.
2. **Search locally** — inspect the saved text, links, resources, reports, and `graph/graph.json` with Octocode local tools.
3. **Validate live behavior** — hand URLs/selectors/actions to `octocode-chrome-devtools` for buttons, forms, search, pagination, menus, infinite scroll, cookies/storage, screenshots, and network/HAR bodies.
4. **Diagnose mismatches** — if CDP finds zero actionable rows, run `actionability-diagnostics.mjs` to classify blocked, JS-shell, selector mismatch, consent/region, or timing/hydration.
5. **Feed back evidence** — add discovered URLs, API endpoints, HAR/body files, screenshots, or DOM/text back into the corpus and continue.

Fallback policy: direct/static → Chrome/CDP read-only validation/fetch → hosted anti-bot provider only with user-approved need and scope.

## What agents should look at

- `AGENT_INDEX.json` — start here.
- `graph/graph.json` — pages, links, actions, resources, risks, selectors, evidence.
- `text/*.clean.part-*.md` — readable page text.
- `extracts/` — links, forms, buttons, tables, resources, metadata.
- `reports/` — summaries, failures, crawl notes, provider costs.
- `raw/` — audit/debug only; do not paste raw HTML into chat.

## Common tasks

| Need | Route |
|---|---|
| Plain page extraction | direct/curl-like fetch, then local search |
| Large website map | bounded crawl, graph navigation, route dedupe |
| JS-rendered or dynamic UI | Chrome/CDP validation |
| Search box/button/form/pagination proof | scrape graph → CDP actionability check |
| Blocked/thin static output | CDP diagnostics; ask before hosted anti-bot escalation |
| API discovery | CDP network/HAR/body capture, then analyze local artifacts |
| Better hosted results | add `SCRAPING_ANT`; see [`docs/PROVIDERS.md`](docs/PROVIDERS.md) |

## Safety defaults

- Public/static first.
- Ask before auth/session cookies, CAPTCHA/MFA, personal data export, anti-bot escalation, high-volume crawling, form submission, purchases/sends/deletes/account changes.
- Secret-like provider parameters are rejected; keys stay in Octocode env and are never printed.
- Large payloads are saved to files and searched locally instead of pasted into chat.

## Docs

- [`docs/PROVIDERS.md`](docs/PROVIDERS.md) — `SCRAPING_ANT`, direct provider, fallback routing.
- [`docs/ADDING_A_VENDOR.md`](docs/ADDING_A_VENDOR.md) — vendor contract for adding providers.
- [`references/browser-scraping.md`](references/browser-scraping.md) — static scrape ↔ Chrome/CDP handoff.
- [`references/route-selection.md`](references/route-selection.md) — choose direct, CDP, or hosted provider safely.
- [`references/failure-recovery.md`](references/failure-recovery.md) — blocked/thin/partial result handling.
