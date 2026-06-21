# CDP Intent Router

Use this file to choose intent(s) and load only the matching detail file. Do not load every intent detail file by default.

## Loading Rule

1. Pick one to three intents from the index below.
2. Load only the detail file(s) listed for those intents.
3. If a detail section points to `SCRIPT_PATTERNS.md`, load only the named pattern.
4. Enable the union of required domains, attach listeners before navigation/action, then run the focused script.
5. For exact CDP params/events, verify `CDP_AGENT_REFERENCE.md` or the official per-domain protocol page.
6. Treat code snippets as adaptable examples. Verify current Web APIs and feature-detect inside the page before using optional browser APIs.

## Combining Intents

Intents are composable. Most real tasks need 2-3 combined. Enable the **union of domains** for all selected intents and merge their event listeners.

| Common combination | Intents | When to use |
|---|---|---|
| Automate + debug | `automate` + `debug` | "do X and tell me what breaks" |
| Login + network | `login` + `network` | "log in and capture what API calls are made" |
| Login + security | `login` + `security` | "log in and audit cookies/tokens after auth" |
| User-auth + scrape | `user-auth` + `scrape` | "let me sign in, then extract data from the authenticated page" |
| User-auth + debug | `user-auth` + `debug` | "let me sign in, then investigate what is breaking after auth" |
| User-auth + security | `user-auth` + `security` | "let me sign in, then audit the session tokens and cookies" |
| Automate + screenshot | `automate` + `screenshot` | "fill the form and take a screenshot of the result" |
| Automate + performance | `automate` + `performance` | "click through the flow and measure render time" |
| Scrape + emulate | `scrape` + `emulate` | "extract data as mobile viewport" |
| Debug + security | `debug` + `security` | "investigate the crash and check what tokens are exposed" |
| Inject + debug | `inject` + `debug` | "patch the function before load, then observe behavior" |
| Full audit | all of the above | "audit everything" |

When combining: enable all required domains first, attach all event listeners, then navigate/interact, then run inspection checks.

## Detail Files

| File | Intents | Load when |
|---|---|---|
| `INTENTS_DEBUG.md` | `debug`, `network`, `console`, `performance`, `memory`, `dom`, `css-coverage`, `js-coverage` | Load this file when the selected intent is about errors, diagnostics, performance, DOM/CSS inspection, or code coverage. |
| `INTENTS_INSPECT.md` | `security`, `websocket`, `service-worker`, `workers`, `intercept`, `screenshot`, `accessibility`, `supply-chain`, `full-audit` | Load this file when the selected intent targets browser subsystems, capture surfaces, security, workers, request interception, or full audits. |
| `INTENTS_AUTOMATION.md` | `automate`, `scrape`, `live-page` | Load this file when the selected intent needs browser interaction, scraping, or a live user-driven page. |
| `INTENTS_AUTH.md` | `login`, `user-auth` | Load this file when the selected intent needs login automation, manual user authentication, or an authenticated browser session. |
| `INTENTS_ENVIRONMENT.md` | `emulate`, `inject`, `monitor` | Load this file when the selected intent changes browser environment, injects instrumentation, or monitors a page over time. |
| `INTENTS_STORAGE_CONSENT.md` | `storage`, `consent` | Load this file when the selected intent audits browser storage, cookies, tokens, quotas, service-worker state, or consent/tracker behavior. |

## Intent Index

| User says... | Intent | Details |
|---|---|---|
| debug, what's wrong, broken, fix this, investigate, agent loop, observe, why is X not working | `debug` | [INTENTS_DEBUG.md#debug](INTENTS_DEBUG.md#debug) |
| automate, do X, click, type, fill, submit, flow, interact, perform steps | `automate` | [INTENTS_AUTOMATION.md#automate](INTENTS_AUTOMATION.md#automate) |
| login, sign in, authenticate, enter credentials, log me in | `login` | [INTENTS_AUTH.md#login](INTENTS_AUTH.md#login) |
| let me log in myself, I'll auth, manual login, open browser so I can sign in, auth flow, open visible browser, I need to authenticate first | `user-auth` | [INTENTS_AUTH.md#user-auth](INTENTS_AUTH.md#user-auth) |
| scrape, extract, collect data, pull content, harvest | `scrape` | [INTENTS_AUTOMATION.md#scrape](INTENTS_AUTOMATION.md#scrape) |
| emulate, mobile, device, throttle, offline, slow network, geolocation | `emulate` | [INTENTS_ENVIRONMENT.md#emulate](INTENTS_ENVIRONMENT.md#emulate) |
| inject, patch, override, hook, intercept before load, monkey-patch | `inject` | [INTENTS_ENVIRONMENT.md#inject](INTENTS_ENVIRONMENT.md#inject) |
| monitor, watch, poll, check every N seconds, keep watching | `monitor` | [INTENTS_ENVIRONMENT.md#monitor](INTENTS_ENVIRONMENT.md#monitor) |
| network, requests, 4xx, API calls, traffic | `network` | [INTENTS_DEBUG.md#network](INTENTS_DEBUG.md#network) |
| console, errors, exceptions, crashes, JS error | `console` | [INTENTS_DEBUG.md#console](INTENTS_DEBUG.md#console) |
| slow, performance, metrics, long task, fps, render | `performance` | [INTENTS_DEBUG.md#performance](INTENTS_DEBUG.md#performance) |
| memory, leak, heap, detached nodes, retained | `memory` | [INTENTS_DEBUG.md#memory](INTENTS_DEBUG.md#memory) |
| DOM, elements, structure, HTML, rendering | `dom` | [INTENTS_DEBUG.md#dom](INTENTS_DEBUG.md#dom) |
| CSS, styles, unused rules, coverage | `css-coverage` | [INTENTS_DEBUG.md#css-coverage](INTENTS_DEBUG.md#css-coverage) |
| JS coverage, dead code, unused functions | `js-coverage` | [INTENTS_DEBUG.md#js-coverage](INTENTS_DEBUG.md#js-coverage) |
| security, cookies, tokens, headers, CSP, exfil | `security` | [INTENTS_INSPECT.md#security](INTENTS_INSPECT.md#security) |
| websocket, WS, real-time, socket frames | `websocket` | [INTENTS_INSPECT.md#websocket](INTENTS_INSPECT.md#websocket) |
| service worker, SW lifecycle, cache, offline, PWA worker | `service-worker` | [INTENTS_INSPECT.md#service-worker](INTENTS_INSPECT.md#service-worker) |
| web worker, shared worker, worker thread, background thread | `workers` | [INTENTS_INSPECT.md#workers](INTENTS_INSPECT.md#workers) |
| intercept, mock, block, fake response, modify request | `intercept` | [INTENTS_INSPECT.md#intercept](INTENTS_INSPECT.md#intercept) |
| screenshot, capture, visual, PDF, print | `screenshot` | [INTENTS_INSPECT.md#screenshot](INTENTS_INSPECT.md#screenshot) |
| accessibility, a11y, aria, screen reader | `accessibility` | [INTENTS_INSPECT.md#accessibility](INTENTS_INSPECT.md#accessibility) |
| third-party, external scripts, CDN, supply chain | `supply-chain` | [INTENTS_INSPECT.md#supply-chain](INTENTS_INSPECT.md#supply-chain) |
| full audit, all checks, everything | `full-audit` | [INTENTS_INSPECT.md#full-audit](INTENTS_INSPECT.md#full-audit) |
| storage, cookies, localStorage, sessionStorage, IndexedDB, quota, cache state | `storage` | [INTENTS_STORAGE_CONSENT.md#storage](INTENTS_STORAGE_CONSENT.md#storage) |
| consent, GDPR, privacy banner, tracker pre-firing, pre-consent cookies | `consent` | [INTENTS_STORAGE_CONSENT.md#consent](INTENTS_STORAGE_CONSENT.md#consent) |
| open page, browse with monitoring, watch this page, live check, open and monitor, open and inspect | `live-page` | [INTENTS_AUTOMATION.md#live-page](INTENTS_AUTOMATION.md#live-page) |

## Output Prefix Reference

| Prefix | Intent categories | Meaning |
|---|---|---|
| `[NETWORK]` | network, security, supply-chain, websocket | HTTP request/response |
| `[NETWORK_ERROR]` | network, security | 4xx/5xx status |
| `[NETWORK_FAILED]` | network | Blocked/failed request |
| `[CONSOLE:ERROR]` | console | `console.error()` call |
| `[CONSOLE:WARN]` | console | `console.warn()` call |
| `[EXCEPTION]` | console | Uncaught JS exception |
| `[EXCEPTION_LOCATION]` | console | Stack frame of exception |
| `[LOG:ERROR]` | console | Browser Log domain error |
| `[PERFORMANCE]` | performance, memory | Metric value |
| `[DOM]` | dom, accessibility | DOM structure info |
| `[CSS]` | css-coverage | CSS rule info |
| `[SECURITY]` | security, supply-chain | Security-specific finding |
| `[METRIC]` | all | Summary count or measurement |
| `[SCREENSHOT]` | screenshot | Path to saved file |
| `[FINDING]` | all | Actionable issue — emit for user-relevant findings |
| `[AUTOMATE]` | automate, login | Step executed in a flow |
| `[AUTH]` | user-auth | Auth polling status update (progress line) |
| `[AUTH_COMPLETE]` | user-auth | Authentication detected — agent may proceed |
| `[AUTH_TIMEOUT]` | user-auth | Timed out waiting for auth — agent must handle |
| `[SCRAPE]` | scrape | Extracted data item |
| `[REASON]` | scrape, automate, debug | Why this step/result leads to the next step |
| `[EMULATE]` | emulate | Environment override active |
| `[INJECT]` | inject | Script injected into document |
| `[MONITOR]` | monitor | State snapshot from polling loop |
| `[SW]` | service-worker | Service Worker lifecycle event or state |
| `[WORKER]` | workers | Worker target attached or event from worker session |
| `[ACTION]` | debug, automate | Concrete next step for agent or developer |
