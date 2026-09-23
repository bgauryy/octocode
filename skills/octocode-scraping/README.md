# Octocode Scraping

Fetch public web pages into a local, cited corpus and query them with clasify — without reading raw bodies into chat.

---

## Use when

| Situation | Use |
|---|---|
| Scrape or crawl public URLs, docs, tables, pricing pages | ✅ This skill |
| Extract structured data (links, forms, headings, JSON-LD) from a static page | ✅ This skill |
| Triage many pages before reading any — route by content type | ✅ This skill (clasify SCREEN) |
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
4. **SCREEN** — clasify all corpus pages *before* reading any body (prevents context bloat)
   - Drop 0-byte files (they return `classificationContextEmpty`) and duplicate pages first
   - ≤ 25 cells per matrix (split across root `queries[]`); omit `maxChars` so parts are not truncated
   - Accept route choices only when `confidence >= 0.9`; `insufficient` (auto-added to every Choice) or `partial` → `consider`
   - On `consider`: `corpus-run --regex <term> --flags i` for file/line, then read or re-clasify that line window
5. **LITERAL VERIFY** — after SCREEN, run `corpus-run.mjs --regex <pattern>` to confirm target data is present in the text; zero matches + `has-target-data >= 0.8` → escalate to CDP
6. **CITE** — report artifact paths + URL metadata; never paste raw HTML into chat

---

## When static isn't enough — CDP escalation

If `corpus-run --regex` returns zero matches for your target data (e.g. fee percentages, code samples) but clasify scored `has-target-data >= 0.8`:

1. Load `octocode-chrome-devtools`
2. Run `open-browser.mjs` + `page-snapshot.mjs` + `dom-operations-check.mjs` on the same URL
3. Bridge results back: `scripts/har-ingest.mjs --session-dir <your-existing-session>`
4. Resume SCREEN on the merged corpus — **do not create a new session**

---

## Clasify question templates (SCREEN)

Always include for web research:

- **`content-type`** (Choice): `pricing-table` / `api-reference` / `docs-guide` / `marketing` / `listing` / `other`
- **`has-target-data`** (Noul): one specific question per goal, e.g. *"Does this contain Stripe fee percentages?"*
- **`route`** (Choice): `read` / `consider` / `skip` / `cdp-needed`
- **`has-text-bodies`** (Noul, on `cdp-network.jsonl`): *"Do any entries have HTML/JSON bodies — not just images or analytics?"*
- **Link routing** (on `links.jsonl` as `context.value`): `crawl-section` / `spot-check` / `stop`

> **Note:** clasify does semantic routing by topic/type. It cannot verify literal presence. Use `corpus-run --regex` for that.

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
