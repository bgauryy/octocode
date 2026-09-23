# Octocode Chrome DevTools

Collect live Chrome DevTools Protocol (CDP) evidence: DOM actionability, network HAR, console, performance, storage, and authenticated pages — without reading raw bodies into chat.

---

## Use when

| Situation | Use |
|---|---|
| Page content requires a real browser to render (JS-heavy SPA) | ✅ This skill |
| You need to click, fill a form, or interact with a live page | ✅ This skill |
| You need HAR capture, console errors, or live network monitoring | ✅ This skill |
| You need DOM element references for automation (CTA buttons, nav links) | ✅ This skill |
| Page is behind login, cookies, or session state | ✅ This skill |
| Scraping static public pages or building a reusable corpus | ❌ → `octocode-scraping` |
| Bulk crawl of a docs site for later querying | ❌ → `octocode-scraping` |
| You already have a static fetch and just need clasify triage | ❌ → `octocode-scraping` |

**Rule of thumb:** if you need a running Chrome instance, use this skill. For plain HTTP-fetchable content and corpus building, use `octocode-scraping`.

## Not for

- Static public pages or bulk crawling → `octocode-scraping`
- Content reachable by a plain HTTP request → `octocode-scraping`

---

## Workflow

```
OPEN/ATTACH → STEALTH → PICK ONE INTENT → RUN(CDP) → REUSE PORT/TAB → SCREEN → QUERY DISK → CLEANUP
```

1. **OPEN** — `scripts/open-browser.mjs --headless --port 9222 --url <url>` → emits `BROWSER_READY` only; does **not** capture page content
2. **RUN** — `scripts/cdp-sandbox.mjs <check-script.mjs> --port 9222` (sequentially — never two in parallel on the same port)
3. **SCREEN** — clasify all captured artifacts *before* reading any body
   - Drop 0-byte files (they return `classificationContextEmpty`); ≤ 25 cells per matrix; omit `maxChars`
   - Accept route choices only when `confidence >= 0.5`; `insufficient` (auto-added to every Choice) or `partial` → `consider`
   - Tailor questions to artifact type (see table below)
4. **QUERY DISK** — read only clasify-approved artifacts
5. **CLEANUP** — `scripts/open-browser.mjs --cleanup --port 9222`

---

## Clasify question templates by artifact type

### `page-snapshot.json` (DOM — `{url, refs:{e1:{role,name},...}}`)

- **`has-product-nav`** (Noul): *“Does the DOM contain 5+ named product/section navigation links?”*
- **`has-cta`** (Noul): *“Does the DOM contain operable CTA buttons — sign-up, start, get-started?”*
- **`has-pricing-elements`** (Noul): *“Does the DOM contain pricing table rows or plan names?”*
- **`dom-intent`** (Choice): `extract-nav-links` / `click-cta` / `extract-pricing` / `inspect-only`

> `dom-check.json` is a **single-element** record — ask only element-level questions (visible, operable, stable). Do not ask whole-DOM questions on it.

### `live-network.har` / `cdp-network.jsonl`

- **`has-text-bodies`** (Noul): *“Any HTML/JSON bodies worth extracting? Score 0.2 or lower if all are images, fonts, or analytics.”*
- **`has-api-calls`** (Noul): *“Any XHR/fetch calls to real API endpoints returning structured JSON?”*
- **`har-action`** (Choice): `extract-bodies` / `navigate-more` / `replay-api` / `skip`

### `network-summary.json`

- **`has-slow-requests`** (Noul): *“Any requests exceeding 2s?”*
- **`has-failures`** (Noul): *“Any non-analytics failed requests indicating a blocked resource or auth wall?”*

### `graph-actionability.json` navigation nodes (as `context.value`)

- **`link-relevance`** (Noul) + **`link-action`** (Choice: `follow` / `spot-check` / `skip`)

---

## CDP safety rules

- **Never run two `cdp-sandbox.mjs` calls in parallel on the same port** — causes silent `exit 0` with no artifact (`CDP error [-32000]: Another locale override`)
- Run CDP checks **sequentially** on a shared port with `--keep-tab`
- `open-browser.mjs` emits `BROWSER_READY` only — follow with `cdp-sandbox.mjs` to capture content

---

## Bridge back to scraping

After CDP capture, hand off to `octocode-scraping`:

```bash
# Merge CDP data into existing scraping corpus session
node <octocode-scraping>/scripts/har-ingest.mjs --session-dir <existing-session-dir>
```

Then resume the scraping SCREEN/CITE pipeline on the merged session. Do not create a new session.

---

## Install

Requires Chrome and Node.js 24+. Sandboxed `--allow-net` needs Node.js 25+.

```bash
npx -y octocode skill install octocode-chrome-devtools
```

## Quick start

```bash
# 1. Open browser
node scripts/open-browser.mjs --headless --port 9222 --url https://example.com

# 2. Run a check (sequential — not parallel)
node scripts/cdp-sandbox.mjs scripts/cdp-checks/page-snapshot.mjs --port 9222 --keep-tab
node scripts/cdp-sandbox.mjs scripts/cdp-checks/graph-actionability-check.mjs --port 9222 --keep-tab

# 3. Cleanup
node scripts/open-browser.mjs --cleanup --port 9222
```

Ready-made checks: `references/cdp-checks.md`.

---

## Related skill

`octocode-scraping` — for static fetch, corpus building, and clasify SCREEN pipeline.
Bridge: `har-ingest.mjs` in the scraping skill merges CDP data into its corpus.
