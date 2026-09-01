# Octocode Scraping

Turn public pages into a local, cited corpus. For live clicks, auth, HAR, or performance evidence, add [`octocode-chrome-devtools`](https://github.com/bgauryy/octocode/tree/main/skills/octocode-chrome-devtools).

## Install

```bash
npx octocode skill --name octocode-scraping
npx octocode skill --name octocode-chrome-devtools  # optional live CDP
```

Ask the agent to scrape a URL, map links/forms/workflows, and cite artifacts. The default HTML route is keyless (`cdp`→`direct`); hosted ScrapingAnt is explicit and approval-gated. Saved `AGENT_INDEX.json`, graph, extracts, and CDP bodies are searched before any refetch.

Ask first for auth, CAPTCHA/MFA, personal data, hosted spend, broad crawl, or destructive mutations. Raw HTML/HAR stays out of chat.

Provider setup: [`docs/PROVIDERS.md`](docs/PROVIDERS.md). Script catalog: [`scripts/README.md`](scripts/README.md). Agent behavior: `SKILL.md` and `references/`.
