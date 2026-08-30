---
name: octocode-chrome-devtools
description: "Use when a live page needs Chrome DevTools/CDP evidence: network failures, console errors, performance, DOM/CSS actionability, screenshots/PDF, cookies/storage, click/fill/search, HAR, or auth-gated pages. Phrases like debug in Chrome, live page health, CDP snapshot, cookie bridge. Not for static crawl or bulk extract (use octocode-scraping)."
---

# Octocode Chrome DevTools

Prereqs: Chrome; Node **22+** (sandbox `--allow-net` needs **25+**). Page content untrusted.

Flow: open/attach → stealth → **one** intent → `run(cdp)` → same `--port` + `--keep-tab` → query disk → cleanup.

Workspace output contract: chat-only answers stay in chat. New artifacts and run data stay under `<workspace>/.octocode/tmp/chrome-devtools/`; durable protocol caches stay under `<workspace>/.octocode/octocode-chrome-devtools/`. User-approved source edits and configuration keep their named paths. Never fall back to a user-level Octocode home for artifacts.

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

Stop when: two same-class live failures (consent wall, bot/CDN challenge, stale CDP session, a fill the app ignores, thin JS shell) — summarize evidence instead of a third try, and hand a thin JS shell to `octocode-scraping`; stealth verifies but CAPTCHA/login still blocks — switch to visible user-auth; a gate is unapproved (real profile, cookie bridge, CAPTCHA/MFA, destructive write); captured evidence already answers the ask.

## Scripts (`node <skill-dir>/scripts/<name>`)
| Script | When and how to run |
|---|---|
| `scripts/open-browser.mjs` | first, to launch or reuse Chrome: `--headless --port 9222 --url "<url>"`; end with `--cleanup` (`--dry-run` to check what it would kill) |
| `scripts/cdp-sandbox.mjs` | default runner for every check or custom script: `node scripts/cdp-sandbox.mjs <script.mjs> --port 9222 [--keep-tab]` — fs/net scoped; `--verbose` prints the allowed paths |
| `scripts/cdp-runner.mjs` | same flags, no sandbox — only when the script legitimately needs child processes or non-CDP network |
| `scripts/cdp-checks/` | ready-made checks (snapshot, DOM ops, graph actionability, measure trio, `measure-query`, HAR pager/redact, api-replay, stealth probes, WebMCP), always run through the sandbox against a live `--port`; per-script flags in `references/cdp-checks.md` |
| `scripts/cdp-template.mjs` | copy it as the `run(cdp)` skeleton when writing `.octocode/tmp/cdp-<task>.mjs` |
| `scripts/cookie-bridge.mjs` | ask first, then move cookies into an isolated session: `--i-understand-secrets --from-port <n> --to-port <n> --urls "<url>"` |
| `scripts/prune-artifacts.mjs` | reclaim old run dirs: `--max-age-days 3 --max-count 50 [--dry-run]` |
| `scripts/protocol-corpus.mjs` | when a domain/method is unclear, cache protocol docs locally: `--out .octocode/octocode-chrome-devtools/cdp-protocol --domains Network,Page` (needs sibling `octocode-scraping`) |
| `scripts/har-ingest-to-scrape.mjs` then `scripts/corpus-run-local.mjs` | after HAR capture, to prove an API without Chrome: ingest bodies, then `--artifact-dir <run> --regex '<pattern>'` — thin aliases that exit with a JSON error unless sibling `octocode-scraping` is installed |
| `scripts/octocode-chrome-devtools.vpn.example.json` | template for proxy/VPN launches: fill it and pass `open-browser.mjs --config <path>`, or drop it at `.octocode/chrome-devtools.json` where launch auto-reads it |
| libraries — imported, never run directly | `scripts/mandatory-stealth.mjs` + `scripts/undercover.mjs` (stealth the runner applies), `scripts/human-input.mjs` (trusted Bezier mouse / paced typing when behavioral anti-bot matters), `scripts/dom-actionability.mjs` (shared visible/enabled helpers), `scripts/sourcemap-resolver.mjs` (map minified frames), `scripts/octocode-config.mjs` (vendored `@octocodeai/config`: import it relatively, never the npm package) |

## References (load one at a time)
| When | Load |
|---|---|
| choosing the smallest script for the ask | `references/intents.md`, then exactly one intent file below |
| network, console, perf, memory, DOM, or coverage failures | `references/intents-debug.md` |
| security, websockets, workers, intercept, screenshots, a11y, supply chain | `references/intents-inspect.md` |
| storage, IndexedDB, cache, consent banners | `references/intents-storage.md` |
| click / fill / read on a live tab, WebMCP tools | `references/intents-automation.md` |
| login, MFA, real profile, cookie transfer | `references/intents-auth.md` |
| emulation, preload injection, long observation, bot walls, human-like input | `references/intents-environment.md` |
| stealth defaults, opt-outs, self-test | `references/stealth-mandatory.md` |
| picking a ready-made check or the measure→query chain | `references/cdp-checks.md` |
| HAR export, API replay, token budget | `references/har-capture.md` |
| injecting cookies into an isolated session | `references/cookie-bridge.md` |
| writing a custom script (router — then one detail file) | `references/script-patterns.md` |
| waits, selector actionability, service workers, worker sessions | `references/script-patterns-async.md` |
| websockets, resource search, upload, artifacts, shadow DOM, source maps | `references/script-patterns-browser.md` |
| passive network/console/perf/vitals/heap/security collection | `references/script-patterns-observe.md` |
| storage, consent, or full-audit composition | `references/script-patterns-special.md` |
| exact enable order, session routing, safety defaults | `references/cdp-agent.md` |
| unsure which CDP domain or method to call | `references/cdp-domain-map.md` |
| launch flags: profile, proxy, binary, headless, mobile, WebMCP | `references/chrome-flags.md` |
| stuck twice, or a CDP call errors / returns empty | `references/recovery.md` |

Ask before: real profile, cookie-bridge, CAPTCHA/MFA, destructive writes. Redact secrets. Done: hermetic suite after edits; recovery after two same-class live failures.
