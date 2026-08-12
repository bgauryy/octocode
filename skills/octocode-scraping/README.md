# Octocode Scraping Skill

Public pages → local cited corpus. Live clicks/HAR/perf → [`octocode-chrome-devtools`](https://github.com/bgauryy/octocode/tree/main/skills/octocode-chrome-devtools).

## Install
```bash
npx octocode skill --name octocode-scraping
npx octocode skill --name octocode-chrome-devtools   # for live CDP
```
Optional hosted key: [`docs/PROVIDERS.md`](docs/PROVIDERS.md) (`SCRAPING_ANT`) — not the default.

## Ask the agent
> Scrape `<URL>` into a corpus, map links/forms/workflows, cite artifacts. If thin/JS-only, validate in Chrome and ingest HAR — ask before ScrapingAnt.

Keyless html: omit `--provider` (`cdp`→`direct`). Search `AGENT_INDEX.json` / graph / extracts before refetch. Bridge: `har-ingest` → `corpus-run`.

## Safety
Ask first: auth, CAPTCHA/MFA, personal data, hosted spend, high-volume crawl, destructive writes. No raw HTML/HAR in chat.

Agent truth: `SKILL.md` + `references/`. Scripts: `scripts/README.md`. Done gate: `eval-benchmark-suite.mjs`.
