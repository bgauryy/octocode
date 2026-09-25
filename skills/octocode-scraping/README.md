# Octocode Scraping

Fetch public web pages into a local, cited corpus and query them with clasify — without reading raw bodies into chat.

---

## Use when

| Situation | Use |
|---|---|
| Scrape or crawl public URLs, docs, tables, pricing pages | ✅ This skill |
| Extract structured data (links, forms, headings, JSON-LD) from a static page | ✅ This skill |
| Triage ambiguous unread pages when metadata and snippets do not settle the next read | ✅ This skill (optional clasify SCREEN) |
| Page renders in the browser but has no content when fetched statically | ❌ → `octocode-chrome-devtools` |
| You need to click, fill, log in, or interact with a live page | ❌ → `octocode-chrome-devtools` |
| You want network HAR, console errors, or live DOM state | ❌ → `octocode-chrome-devtools` |

**Rule of thumb:** if `curl` or a headless request returns the content you need, use this skill. If a real browser is required, use `octocode-chrome-devtools`.

## Not for

- JS-rendered pages, live DOM interaction, or authenticated sessions → `octocode-chrome-devtools`
- DOM automation, HAR network capture, or console monitoring → `octocode-chrome-devtools`

---

## Workflow

```
FRAME → POLICY → ROUTE → FETCH → CORPUS → SCREEN → CITE → RECOVER
```

1. **FRAME** — set URL, goal, depth, output path before fetching
2. **FETCH** — `scripts/fetch.mjs` (or `fetch-and-brief.mjs` for a quick summary)
3. **CORPUS** — pages land in `.octocode/tmp/scrape/<sessionId>/text/`, extracts in `/extracts/`
4. **SCREEN, when useful** — use metadata and snippets first; clasify ambiguous unread pages when it changes the next read. See [references/clasify-screen.md](references/clasify-screen.md)
5. **LITERAL VERIFY** — inspect the selected source and use `corpus-run.mjs --regex <pattern>` for exact text checks
6. **CITE** — report artifact paths + URL metadata; never paste raw HTML into chat

---

## When static isn't enough — CDP escalation

If the target data still appears absent after checking wording and extraction quality, and the page appears JS-rendered:

1. Load `octocode-chrome-devtools`
2. Run `open-browser.mjs` + `page-snapshot.mjs` + `dom-operations-check.mjs` on the same URL
3. Bridge results back: `scripts/har-ingest.mjs --session-dir <your-existing-session>`
4. Inspect the merged corpus; screen ambiguous unread captures if useful. **Do not create a new session**

---

## Install

```bash
npx -y octocode skill install octocode-scraping
```

See [provider setup](docs/PROVIDERS.md) and the [script catalog](scripts/README.md).

---

## Related skill

`octocode-chrome-devtools` — for live browser, DOM interaction, HAR capture, and CDP evidence.
Bridge: `scripts/har-ingest.mjs` merges CDP data into this skill's corpus.
