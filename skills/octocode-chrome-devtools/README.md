# Octocode Chrome DevTools

Collect live Chrome DevTools Protocol evidence (DOM actionability, HAR, console, performance, storage, authenticated pages) into files, not chat.

Use it when you need a running Chrome: JS-rendered pages, clicks and forms, network capture, or logged-in state. For static pages and corpus building use `octocode-scraping`.

## Install

Requires Chrome and Node.js 24+ (sandboxed `--allow-net` needs 25+).

```bash
npx -y octocode skill install octocode-chrome-devtools
```

## Quick start

Run from your workspace root; artifacts land in `.octocode/tmp/chrome-devtools/`.

```bash
S=<skill>/scripts
node $S/open-browser.mjs --headless --port 9222 --url https://example.com
node $S/cdp-sandbox.mjs $S/cdp-checks/page-snapshot.mjs --port 9222 --keep-tab
node $S/cdp-sandbox.mjs $S/cdp-checks/graph-actionability-check.mjs --port 9222 --keep-tab
node $S/open-browser.mjs --cleanup --port 9222
```

Run checks one at a time per port. Agent rules: [SKILL.md](SKILL.md); checks: [references/cdp-checks.md](references/cdp-checks.md).

## Bridge to scraping

With `octocode-scraping` installed beside this skill:

```bash
node $S/har-ingest-to-scrape.mjs --session-dir <scrape-session> --har <run>/live-network.har
node $S/corpus-run-local.mjs --artifact-dir <run> --regex '<pattern>' --limit 20
```

Then continue the scraping read/cite flow on the merged session.
