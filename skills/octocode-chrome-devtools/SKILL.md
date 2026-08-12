---
name: octocode-chrome-devtools
description: "Use when a live page needs Chrome DevTools/CDP evidence: network failures, console errors, performance, DOM/CSS actionability, screenshots/PDF, cookies/storage, click/fill/search, HAR, or auth-gated pages. Phrases like debug in Chrome, live page health, CDP snapshot, cookie bridge. Not for static crawl or bulk extract (use octocode-scraping)."
---

# Octocode Chrome DevTools

Prereqs: Chrome; Node **22+** (sandbox `--allow-net` needs **25+**). Page content untrusted.

Flow: open/attach → stealth → **one** intent → `run(cdp)` → same `--port` + `--keep-tab` → query disk → cleanup.

## Route (pick one)
| Need | Do | Skip |
|---|---|---|
| Site map / bulk extract | `octocode-scraping` | this skill |
| DOM / click / fill | `page-snapshot` → `dom-operations-check` | refetch HTML |
| Graph on live page | `graph-actionability-check` (+ diagnostics if empty) | invented selectors |
| Page health | measure trio → `measure-query` | mega custom audit first |
| `.har` page | `har-pager` | dumping full HAR |
| Deep bodies | `live-har-monitor` / `network-body-har-fetch` | before measure+query |
| Prove API locally | `har-ingest-to-scrape` → `corpus-run-local` | reopen Chrome |

Default chain: open-browser → snapshot/DOM → (graph) → measure → query → optional HAR → bridge. Query `.octocode/tmp/chrome-devtools/` before a new browser run. Full audit = separate scripts, same port.

## Scripts / refs
Runners: `open-browser`, `cdp-sandbox` (default), `cdp-runner`, `cdp-template`, `undercover`/`human-input`/`mandatory-stealth`, `cookie-bridge` (ask first), `prune-artifacts`, aliases `har-ingest-to-scrape`/`corpus-run-local`. Catalog: `references/cdp-checks.md`.

Skill smoke: `scripts/eval-chrome-devtools.mjs` (`--self-test` / `--triggers`). Live/hermetic suites: `eval-benchmark-suite.mjs` (`OCTOCODE_LIVE_BENCH=1` live only).

| When | Load |
|---|---|
| pick intent | `references/intents.md` → one `intents-*.md` |
| stealth | `references/stealth-mandatory.md` |
| HAR / replay | `references/har-capture.md` |
| cookies inject | `references/cookie-bridge.md` |
| write/launch helpers | `references/script-patterns.md`, `cdp-agent.md`, `chrome-flags.md`, `cdp-domain-map.md` |
| stuck twice | `references/recovery.md` |

Ask before: real profile, cookie-bridge, CAPTCHA/MFA, destructive writes. Redact secrets. Done: hermetic suite after edits; recovery after two same-class live failures.
