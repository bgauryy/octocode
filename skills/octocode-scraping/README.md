# Octocode Scraping

Turn public web pages into a local, cited corpus that supports repeated queries without repeated fetches.

## Use when

- You need to scrape or crawl public pages, documentation, tables, or product fields.
- You must map a site into links, forms, resources, and workflows.
- You need to diagnose blocked, thin, or oversized pages.

Use `octocode-chrome-devtools` for live clicks, authenticated sessions, HAR, or performance evidence.

## Workflow

```text
UNDERSTAND → POLICY GATE → DISCOVER → FETCH → NORMALIZE → INDEX → QUERY → CITE
```

The default HTML route is keyless: local CDP when available, then direct fetch. Hosted ScrapingAnt use requires explicit approval. Query existing corpus, graph, extraction, and captured-body artifacts before refetching.

Ask before authentication; CAPTCHA or MFA handling; personal data; hosted spend; broad crawling; or destructive mutations. Keep raw HTML and HAR files out of chat.

## Install

```bash
npx octocode skill install octocode-scraping --platform codex
```

See [provider setup](docs/PROVIDERS.md) and the [script catalog](scripts/README.md).

## Maintainer verification

Run the `octocode-skills` review against this folder.
