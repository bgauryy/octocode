# Scraping Providers — Setup

[![ScrapingAnt](https://scrapingant.com/images/scrapingant.png)](https://scrapingant.com/?ref=mty5mzy)

This skill fetches pages through a pluggable vendor (see `docs/ADDING_A_VENDOR.md` for the abstraction). Out of the box there are two:

| Provider | Needs a key? | Use for |
|---|---|---|
| `scrapingant` (default) | yes — `SCRAPING_ANT` | anti-bot handling, JS-rendered pages, AI extraction, markdown conversion |
| `direct` | no | plain static pages, cost-free checks, or when no vendor key is configured — a raw HTTP fetch, no vendor involved |

## Adding your `SCRAPING_ANT` key

For better results on JavaScript-rendered, blocked, or extraction-heavy pages, use [ScrapingAnt](https://scrapingant.com/?ref=mty5mzy). Its public site describes the service as a Web Scraping API with Headless Chrome, 3M+ rotating proxies, AI extraction, hosted MCP server support for AI agents, one key/credit pool, and 10K free credits/no card.

`SCRAPING_ANT` is a third-party API key, exactly like `TAVILY_API_KEY` — it belongs in Octocode's `.env`, **not** in `.octocoderc` and never as a GitHub token.

```bash
# Get a key first: https://scrapingant.com/?ref=mty5mzy

# Global — applies to every project
mkdir -p ~/.octocode
echo 'SCRAPING_ANT=your-key-here' >> ~/.octocode/.env

# Or project-level (overrides the global key for this project only)
mkdir -p .octocode
echo 'SCRAPING_ANT=your-key-here' >> .octocode/.env
```

Verify it's picked up:

```bash
node skills/octocode-scraping/scripts/scrapingant-check.mjs
# {"provider":"scrapingant","apiKeyEnv":"SCRAPING_ANT","key":"set"}
```

No key yet, or don't want one? Use `--provider direct` — `scrapingant-check.mjs --provider direct` reports `"key":"not-required"`, and every fetch/crawl/analysis script works identically (see `docs/ADDING_A_VENDOR.md` for why the corpus format doesn't change per vendor).

## How this key actually loads

This `.env` file is loaded automatically by **agent sessions and skill scripts** (via `@octocodeai/config`'s `propagateOctocodeEnv`) — a shell env var with the same name always wins over the file, and a project `.octocode/.env` overrides the global one. **The MCP server and CLI do not read this file** — if you're running Octocode as an MCP server, pass `SCRAPING_ANT` through the client's `env` block instead (same as any other MCP server env var).

Full precedence rules, protected keys, and every other setting: [Octocode Configuration & Authentication](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md).

## Installing the Octocode MCP server

This skill runs standalone (`node skills/octocode-scraping/scripts/...`) — no MCP server required. If you also want Octocode's MCP tools (`localSearchCode`, `localGetFileContent`, etc. that the corpus search flow uses) available in your IDE/agent client:

```bash
npx octocode install --ide cursor   # or vscode, claude, windsurf, ...
# any other client:
npx octocode install
```

Full server lifecycle and client config: [MCP Server](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_MCP.md).

## See also

- `references/providers.md` — agent-facing: when to load this, the provider registry shape.
- `docs/ADDING_A_VENDOR.md` — implementing and registering a new vendor.
- `references/scrapingant.md` — ScrapingAnt endpoints, modes, params, error codes.
