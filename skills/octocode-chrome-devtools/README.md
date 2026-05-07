# Chrome DevTools Skill

Use this skill when you want an agent to investigate a website or web app through a real Chrome session.

It is built for browser evidence: network calls, console errors, DOM state, storage, workers, screenshots, authenticated pages, and repeated monitoring.

Use it when normal browser automation cannot explain what Chrome actually saw.

## Prerequisites

For normal chat usage, you usually only need:

- Chrome installed locally
- Node.js available for the bundled launcher/runner scripts
- a URL, local app route, or already-open browser page to inspect

Optional setup:

- proxy/VPN routing through `.octocode/chrome-devtools.json`
- a visible browser session for login, MFA, CAPTCHA, or live-page inspection
- Octocode local/GitHub tools when you want to trace browser evidence back to source code

## Start Here

Ask naturally. Include the URL, the behavior you expect, and the signal you care about.

Good prompts:

- "Open this page and debug why submit fails."
- "Watch network calls when I click checkout."
- "Open a visible browser, I'll log in, then inspect API errors."
- "Detect whether this page is blocking bots or showing a CAPTCHA."
- "Run the auth flow, pause for MFA/CAPTCHA if needed, then inspect the app."
- "Run a storage and consent audit on this site."
- "Scrape table rows from the authenticated page."
- "Monitor this page every 30 seconds for exceptions."

What the agent should do:

- open or attach to Chrome
- choose one or two focused intents
- run a narrow CDP script
- report prefixed evidence such as `[FINDING]`, `[NETWORK_ERROR]`, `[EXCEPTION]`, or `[REASON]`
- reuse the same tab/session on follow-up checks when possible

## Prompt To Intent Cheatsheet

Use this when you know what you want but not which intent name to ask for.

| You want... | Ask for... | Typical intents |
|---|---|---|
| "Something is broken; find why" | debug this flow/page | `debug`, `network`, `console` |
| "Which API call fails?" | inspect network while I do X | `network`, `automate` |
| "I need to log in first" | open visible browser and wait for me | `user-auth`, `debug` |
| "Log me in automatically" | run the login flow | `login`, `network`, `security` |
| "Click/fill/navigate" | automate this browser flow | `automate`, `debug` |
| "Extract data" | scrape these fields/rows | `scrape`, optionally `user-auth` |
| "Why is it slow?" | measure performance for this action | `performance`, `automate`, `emulate` |
| "Possible memory leak" | run a memory loop | `memory` |
| "Layout or element state is wrong" | inspect DOM/CSS/accessibility | `dom`, `accessibility`, `screenshot` |
| "Storage/cookies/consent" | audit browser storage and consent | `storage`, `consent`, `security` |
| "Worker, cache, or offline bug" | inspect service workers/workers/cache | `service-worker`, `workers`, `storage` |
| "Real-time/socket issue" | inspect WebSocket frames | `websocket`, `network` |
| "Block/mock a request" | intercept this request | `intercept`, `debug` |
| "Capture visual proof" | take screenshot/PDF | `screenshot`, optionally `automate` |
| "Bot wall or CAPTCHA" | detect challenge flow | `debug`, `security`, optionally `inject` once |
| "Trace browser error to code" | debug, then source trace | `console`, source maps, Octocode tools |

## When To Use It

Use this skill when you need browser evidence, not guesses:

- debug runtime failures from real page behavior
- inspect requests, responses, and failed network calls
- inspect DOM, CSS, storage, cookies, workers, and service workers
- run interaction flows such as click, type, navigate, wait, and extract
- capture screenshots and PDF output
- inspect authenticated pages after manual login
- monitor a page over time
- route browser traffic through proxy/VPN endpoints when configured

Use a different tool when:

- you need production-grade E2E tests, assertions, retries, and cross-browser coverage: use Playwright
- you only need simple click/fill/navigation from accessibility snapshots: Chrome DevTools MCP or Playwright MCP may be faster
- you need managed scraping infrastructure, proxies, CAPTCHA services, or hosted browsers: use a dedicated browser automation service
- you want a reusable app test suite rather than one-off browser forensics: write tests instead of ad-hoc CDP scripts

## Common Workflows

### Quick Debug

Prompt:

- "Debug why this flow fails after clicking Pay."

Expected output:

- network, console, and exception evidence
- likely root cause and next steps

### Manual Auth Then Inspect

Prompt:

- "Open a visible browser, I'll sign in, then inspect post-login errors."

Expected output:

- the agent pauses for your login
- analysis continues after your confirmation

### Live Page Monitoring

Prompt:

- "Keep this page open and monitor errors every 30s."

Expected output:

- repeated state checks
- diff-like updates as page behavior changes

### Authenticated Scrape

Prompt:

- "After I log in, scrape invoice rows: id, date, amount."

Expected output:

- structured extracted data
- clear scrape findings

### Proxy/VPN Investigation

Prompt:

- "Use configured proxy/VPN and debug this URL."

Expected output:

- browser launched with the configured proxy route
- normal intent execution on top

## OOTB Flows

These are ready-made investigation paths you can ask for without naming CDP internals.

### Bot Wall Detection

Prompt:

- "Check if this page is blocking bots or headless Chrome."

Expected output:

- status codes, redirects, console errors, and visible page signals
- evidence for bot-wall patterns such as challenge pages, blocked resources, or unusual access-denied responses
- a recommendation to retry in visible mode or with configured proxy routing when useful

### CAPTCHA Detection

Prompt:

- "Detect CAPTCHA or challenge flows on this URL."

Expected output:

- CAPTCHA/challenge evidence from DOM text, frames, network calls, and page state
- a clear pause if user solving is required
- no attempt to bypass CAPTCHA automatically

### Auth Flow

Prompt:

- "Open a visible browser, guide me through login, then continue the investigation."

Expected output:

- visible Chrome for manual sign-in
- pause points for MFA, SSO, consent, or CAPTCHA
- post-login tab re-targeting before the agent reads protected state
- redacted auth/storage findings

### Session Reuse Flow

Prompt:

- "Keep this authenticated session open for follow-up checks."

Expected output:

- the same CDP port and tab reused across related checks
- saved tab/resource metadata for follow-up runs
- no reload unless you ask for one

### Anti-Flake Recovery Flow

Prompt:

- "This browser run is flaky; recover and retry the right way."

Expected output:

- retry guidance from `[CDP_RETRY_NEEDED]`
- fresh target listing and re-targeting
- a narrower rerun that changes one meaningful thing

### Performance Flow

Prompt:

- "Measure why this page/action feels slow."

Expected output:

- timing metrics, long tasks, slow resources, and render-related signals
- a narrow comparison before/after one action when the prompt names an interaction
- source-trace hints when a script, route, or package is the likely cause

### WebSocket Flow

Prompt:

- "Inspect the WebSocket messages during this live update."

Expected output:

- WebSocket request metadata and frame summaries
- frame direction, size, timing, and safe message shape
- no secret payload values in the final report

### Service Worker And Cache Flow

Prompt:

- "Check whether the service worker or cache is causing stale/offline behavior."

Expected output:

- service worker registration and lifecycle state
- Cache Storage and relevant fetch behavior
- a recommendation to reload, unregister, clear cache, or inspect source only when evidence supports it

### Visual Capture Flow

Prompt:

- "Capture proof of the broken state after this action."

Expected output:

- screenshot or PDF file path
- the action/state that was captured
- related DOM/network/console evidence when the visual state is caused by runtime behavior

### Source Map Flow

Prompt:

- "Resolve this browser exception to original source."

Expected output:

- generated stack frame and source-map candidate
- original file/line when source maps are available
- local or external Octocode follow-up when the source location points to a repo/package

## Use Case Playbook

Use these as default bundles when a prompt names an outcome instead of CDP internals.

| Use case | Intents | Browser mode | Primary evidence |
|---|---|---|---|
| Page or flow is broken | `debug`, usually `network` + `console` | Headless for public pages, visible for user-driven repro | `[NETWORK_ERROR]`, `[NETWORK_FAILED]`, `[EXCEPTION]`, `[FINDING]`, `[ACTION]` |
| API call fails after an action | `automate` + `network` | New tab with listeners before navigation/action | HTTP status, request URL/method, blocked reason, response timing |
| Authenticated bug | `user-auth` + `debug` or `network` | Visible Chrome, `--keep-tab`, no reload after login unless requested | Post-login console/network/storage metadata with secret values redacted |
| Data extraction from a live app | `user-auth` + `scrape` or `scrape` + `emulate` | Visible for manual login, headless for public pages | `[SCRAPE]` rows, selector/resource notes, `[REASON]` decisions |
| Performance regression | `performance`, optionally `automate` or `emulate` | New tab; attach metrics before navigation/action | `[PERFORMANCE]`, long tasks, timing metrics, slow requests |
| Memory leak suspicion | `memory` | Headless or visible, narrow action loop | Heap metrics, detached-node signals, repeated measurements |
| Layout, DOM, or accessibility issue | `dom`, `accessibility`, optionally `screenshot` | Reuse current tab for live state, new tab for load evidence | `[DOM]`, accessibility tree findings, `[SCREENSHOT]` path |
| Storage, cookie, or consent audit | `storage`, `consent`, optionally `security` | Headless isolated unless real session is explicitly approved | Cookie/storage key names, quota/cache metadata, pre-consent tracker findings |
| Security or supply-chain check | `security`, `supply-chain`, optionally `network` | Headless isolated by default | CSP/header findings, third-party script list, sensitive-key presence without values |
| Worker, service worker, or WebSocket issue | `workers`, `service-worker`, `websocket` | Keep target alive; use session-routed worker commands | `[WORKER]`, `[SW]`, WebSocket frame metadata, cache/offline state |
| Request blocking or mocking | `intercept` + `debug` | New tab; `Fetch.enable` before navigation | Paused request decisions, fulfilled/failed/continued URLs |
| Bot wall or CAPTCHA triage | `debug`, `security`, optionally `inject` once | Headless first; visible user gate if challenge persists | Challenge DOM/frame/network signals, no automatic CAPTCHA bypass |
| Proxy/VPN route investigation | Any selected intent + launcher proxy config | Fresh Chrome session on a clean port | Launcher JSON showing proxy configured, then normal intent evidence |
| Source-traced browser error | `console` + source-map helper, then Octocode source trace | New tab when load-time stacks matter | `[EXCEPTION_LOCATION]`, `[SOURCEMAP]`, source file/line candidate |

## Octocode Integration

Use Chrome DevTools for live browser evidence, then use Octocode tools to connect that evidence to code.

Ask for the combined flow when a browser finding includes a stack trace, source map, route, endpoint, package name, script URL, component selector, or failing request.

Good prompts:

- "Debug this page, then trace the failing request to local code."
- "Find the source for this browser exception in my workspace."
- "Check whether this error comes from our app or an external package."
- "Use Chrome evidence first, then inspect the upstream GitHub repo if it points to a dependency."

### Local Workspace Flow

Use this when the failing app or service code is in the current workspace.

Expected path:

1. Capture browser evidence with `network`, `console`, `debug`, or `performance`.
2. Extract stable clues: URL path, API route, stack frame, source-map location, selector, component name, or package import.
3. Use local Octocode tools to search and navigate code: `localSearchCode` -> `lspGotoDefinition` / `lspFindReferences` -> `localGetFileContent`.
4. Report the browser symptom and the local source location together.

Example output:

- `[NETWORK_ERROR] POST /api/checkout returned 500`
- `[SOURCE_TRACE] Local handler candidate: packages/app/src/routes/checkout.ts`
- `[FINDING] The browser failure maps to the checkout submit handler, not the payment iframe.`

### External Code Flow

Use this when the browser evidence points to a dependency, CDN script, SDK, third-party widget, or upstream repository.

Expected path:

1. Capture the script URL, package name, stack frame, source-map URL, or third-party request.
2. Use Octocode package/GitHub research to inspect external code without relying on guesses.
3. Compare the external behavior with local usage: version, import path, initialization config, and runtime call site.
4. Report whether the issue is likely local integration, dependency behavior, or a remote service response.

Example output:

- `[EXCEPTION] TypeError thrown from vendor checkout SDK`
- `[SOURCE_TRACE] External package candidate: @vendor/checkout-widget`
- `[FINDING] Local code passes an unsupported option; the external SDK rejects it during initialization.`

### Combined Browser + Code Loop

Best DX loop:

1. Reproduce in Chrome.
2. Save evidence prefixes.
3. Trace to local or external source with Octocode.
4. Validate the hypothesis with one focused browser rerun.
5. Summarize both sides: what Chrome observed and what the code explains.

## What Results Look Like

Reports should be short, evidence-first, and prefixed so you can scan them quickly.

Example:

```text
[ACTION] Clicked "Pay" after attaching Network and Runtime listeners.
[NETWORK_ERROR] POST /api/checkout returned 500 in 184ms.
[EXCEPTION] TypeError: Cannot read properties of undefined (reading 'total')
[EXCEPTION_LOCATION] app.checkout.bundle.js:2:91822
[SOURCEMAP] Candidate source: src/checkout/submitOrder.ts:87
[FINDING] The failure is in the submit handler after the payment API response, not in the button click.
[ACTION] Trace /api/checkout or submitOrder.ts with Octocode local tools next.
```

Good reports should include:

- what was triggered
- what Chrome observed
- which evidence is user-relevant
- what to try next
- redaction for secrets and auth/session values

## Safety Gates

The agent should pause and ask before:

- using your real Chrome profile/session
- login, CAPTCHA, or manual UI steps
- destructive actions such as submit, delete, purchase, or send

Sensitive values such as token values, cookie values, passwords, and session secrets should be redacted in outputs.

## Core Features

- Headless and visible browser modes
- Live-page mode: you interact, then the agent inspects current state
- Auth-aware flows with user confirmation gates
- OOTB bot-wall, CAPTCHA detection, auth, session reuse, and recovery flows
- Session-first reuse across many checks/tabs on the same CDP port
- Structured findings through output prefixes
- Step-by-step reasoning logs for scrape and automation loops
- Per-port TMP metadata for smarter follow-up runs
- Retry and recovery hints through `[CDP_RETRY_NEEDED]`
- Proxy/VPN-compatible routing through launch flags or `.octocode` config
- Source-map resolution and Octocode local/external source tracing
- Performance, memory, coverage, worker, service worker, WebSocket, accessibility, screenshot/PDF, security, storage, consent, and supply-chain checks

## Why It Helps

This skill inspects Chrome itself instead of only driving a page like a test runner. It uses Chrome DevTools Protocol directly, so the agent can build focused checks for the exact problem and explain what happened from browser evidence.

The motivation is simple: many browser bugs are invisible from code search alone. Failed requests, console exceptions, service worker state, storage mutations, source-map locations, WebSocket frames, and bot/auth walls only become obvious after observing the real page.

Compared with Playwright MCP, Chrome DevTools MCP, agent-browser, and Puppeteer workflows, users usually get:

- lower setup friction for this skill's scripts
- broad debugging, inspection, automation, auth, and source-trace coverage in one workflow
- stronger iteration through session reuse and per-port metadata
- clearer evidence loops through prefixed output
- safer real-world operation through explicit auth, profile, and destructive-action gates
- practical recovery guidance when CDP calls fail

In practice, this means users get signal faster, rerun less, and keep context while reproducing issues.

## Intent Catalog

Intent router:

- `references/INTENTS.md`

| Intent | What You Get |
|---|---|
| `debug` | Broad investigation for "what is broken" |
| `network` | API traffic, request failures, status errors |
| `console` | Console errors/warnings and JS exceptions |
| `performance` | Performance metrics, long tasks, rendering signals |
| `memory` | Memory and heap leak investigation |
| `dom` | DOM structure/state checks |
| `css-coverage` | Used vs unused CSS |
| `js-coverage` | Used vs unused JS |
| `automate` | Agent performs browser actions |
| `scrape` | Data extraction from page content |
| `live-page` | Keep visible page open, inspect without reload |
| `login` | Agent-led login flow |
| `user-auth` | You log in manually, then agent continues |
| `emulate` | Device, network, and geo-style environment emulation |
| `inject` | Controlled pre-load patching/hooking |
| `monitor` | Repeated checks over time |
| `security` | Cookies, tokens, headers, CSP, and security posture |
| `websocket` | Real-time frame/message inspection |
| `service-worker` | Service worker lifecycle, cache, and offline checks |
| `workers` | Web/shared worker checks |
| `intercept` | Block/mock/modify request paths |
| `screenshot` | Visual capture and PDF-style snapshots |
| `accessibility` | Accessibility-oriented checks |
| `supply-chain` | Third-party/CDN script risk checks |
| `full-audit` | Multi-area broad audit |
| `storage` | Cookies, localStorage, sessionStorage, IDB, and quota |
| `consent` | GDPR/consent and tracker-before-consent behavior |

## Proxy/VPN Support

Chrome flags cannot sign into a VPN provider account directly. They can route browser traffic through proxy endpoints such as HTTP, SOCKS, or PAC.

Common proxy-capable setups:

- NordVPN SOCKS5 endpoints
- Mullvad SOCKS5 endpoints
- PIA SOCKS5 endpoints
- Proton VPN when a local proxy endpoint is configured

Project config file supported by launcher:

- `.octocode/chrome-devtools.json`

Example:

```json
{
  "proxy": {
    "enabled": true,
    "server": "socks5://127.0.0.1:1080",
    "bypassList": "<-loopback>"
  }
}
```

Also see:

- `scripts/octocode-chrome-devtools.vpn.example.json`
- `references/CHROME_FLAGS.md`

## Troubleshooting

If a run fails or is flaky:

1. Ask the agent to inspect retry guidance from `[CDP_RETRY_NEEDED]`.
2. Ask it to re-list and re-target browser tabs.
3. If it is still failing, direct it to `references/RECOVERY.md`.

## Script Inventory

| Script | Purpose | Use it for |
|---|---|---|
| `scripts/open-browser.mjs` | Launch, reuse, or clean up Chrome with CDP enabled | Headless isolated sessions, visible/live sessions, real-profile sessions after approval, proxy/PAC routing |
| `scripts/cdp-sandbox.mjs` | Run generated scripts through Node permissions with isolated output/session metadata | Default execution path for untrusted one-off CDP scripts |
| `scripts/cdp-runner.mjs` | Attach to a target and provide the `run(cdp)` API | Trusted local iteration, target listing, target selection, retry metadata |
| `scripts/cdp-template.mjs` | Baseline `export async function run(cdp)` script | Starting point for network/console/log evidence collection |
| `scripts/sourcemap-resolver.mjs` | Resolve generated JS stack locations through source maps without retaining `sourcesContent` | Source-traced console exception investigations |
| `scripts/undercover.mjs` | Apply and verify headless-fingerprint masking | One guarded retry for public bot-wall triage before visible user gate |
| `scripts/octocode-chrome-devtools.vpn.example.json` | Example proxy config | Copying the shape of `.octocode/chrome-devtools.json` |

## Optional CLI

Run launcher scripts directly when you want to inspect the raw workflow.

```bash
SKILL_DIR="/Users/guybary/Documents/octocode-mcp/skills/octocode-chrome-devtools"
TMPDIR="$(node -e "process.stdout.write(require('os').tmpdir())")"
PORT=9222

node "$SKILL_DIR/scripts/open-browser.mjs" --headless --port "$PORT"
node "$SKILL_DIR/scripts/cdp-sandbox.mjs" --list-targets --port "$PORT"
node "$SKILL_DIR/scripts/open-browser.mjs" --port "$PORT" --cleanup
```

Session metadata location, shared by all runs on the same port:

```bash
$TMPDIR/.octocode-chrome-devtools/session-meta/port-$PORT/
```

Important files:

- `session-metadata.json` - active target, last status, last script
- `targets-latest.json` - latest tab/worker snapshot
- `resource-map.json` - stable mapping for selectors, endpoints, and tab roles
- `reasoning-log.json` - why each step was chosen in long loops
- `run-history.json` - recent run timeline

## Reference Files

- `references/INTENTS.md`
- `references/INTENTS_DEBUG.md`
- `references/INTENTS_AUTOMATION.md`
- `references/INTENTS_AUTH.md`
- `references/INTENTS_ENVIRONMENT.md`
- `references/INTENTS_INSPECT.md`
- `references/INTENTS_STORAGE_CONSENT.md`
- `references/CHROME_FLAGS.md`
- `references/RECOVERY.md`
