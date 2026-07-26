# CDP Intent Router

Load when choosing the smallest CDP script. Why: map user intent to one detail file and stable prefixes.

Pick one primary intent. Combine rows only for broad audits as separate scripts on the same port.

| Intent | When | Detail |
|---|---|---|
| debug/network/console/performance/memory/dom/coverage | diagnose failures or metrics | `references/intents-debug.md` |
| security/websocket/service-worker/workers/intercept/screenshot/accessibility/supply-chain/full-audit | inspect beyond page text | `references/intents-inspect.md` |
| storage/consent | storage, IndexedDB, cache, consent banners | `references/intents-storage.md` |
| automate/scrape/live-page | click/fill/read or attach live | `references/intents-automation.md` |
| login/user-auth/cookie-bridge | manual auth or cookie transfer | `references/intents-auth.md` |
| emulate/inject/monitor | device patches, long observe | `references/intents-environment.md` |
| HAR/Playwright/API-replay | network files, replay, token budget | `references/har-playwright.md` |

Prefixes: `[FINDING]`, `[ACTION]`, `[METRIC]`, `[REASON]`, `[NETWORK_ERROR]`, `[NETWORK_FAILED]`, `[EXCEPTION]`, `[CONSOLE:TYPE]`, `[LOG:LEVEL]`, `[SCREENSHOT]`, `[ARTIFACT]`, `[AUTH_COMPLETE]`, `[AUTH_TIMEOUT]`, `[SOURCEMAP]`.

Full audit: debug/network first, then security/storage/a11y/performance/screenshot as separate scripts.
