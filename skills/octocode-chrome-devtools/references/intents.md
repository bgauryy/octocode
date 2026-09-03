# CDP intent router

Load when choosing the smallest CDP script. Why: one intent → one detail file + stable prefixes.

Pick **one** primary intent. Full audit (only if asked) = separate scripts, same port.

| Intent | When | Detail |
|---|---|---|
| debug/network/console/perf/memory/dom/coverage | diagnose failures or metrics | `references/intents-debug.md` |
| security/websocket/workers/intercept/screenshot/a11y/supply-chain | inspect beyond page text | `references/intents-inspect.md` |
| storage/consent | storage, IndexedDB, cache, banners | `references/intents-storage.md` |
| automate/live-page | click/fill/read on a live URL/tab | `references/intents-automation.md` |
| login/auth/cookie-bridge | manual auth or cookie transfer | `references/intents-auth.md` |
| emulate/inject/monitor | device patches, long observe | `references/intents-environment.md` |
| HAR/API-replay | network files, replay, token budget | `references/har-capture.md` |
| page health | measure trio + `measure-query` | `references/cdp-checks.md` |

Default when unsure: `automate/live-page` or `debug/network`. Static crawl → `octocode-scraping`.

Prefixes: `[FINDING]`, `[ACTION]`, `[METRIC]`, `[REASON]`, `[NETWORK_ERROR]`, `[NETWORK_FAILED]`, `[EXCEPTION]`, `[CONSOLE:TYPE]`, `[LOG:LEVEL]`, `[SCREENSHOT]`, `[ARTIFACT]`, `[AUTH_COMPLETE]`, `[AUTH_TIMEOUT]`, `[SOURCEMAP]`.
