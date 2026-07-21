# CDP Intent Router

Load when choosing the smallest CDP script for a browser task. Why: map user intent to one detail reference and stable output prefixes.

## Loading Rule
Pick one primary intent. Load its detail file only when the router row matches the task; combine rows only for broad audits.

## Intent Routes
| Intent | When | Detail |
|---|---|---|
| debug/network/console/performance/memory/dom/coverage | diagnose browser failures or metrics | `references/INTENTS_DEBUG.md` |
| security/websocket/service-worker/workers/intercept/screenshot/accessibility/supply-chain/full-audit | inspect browser surfaces beyond page text | `references/INTENTS_INSPECT.md` |
| storage/consent | inspect local storage, IndexedDB, cache, or consent banners | `references/INTENTS_STORAGE_CONSENT.md` |
| automate/scrape/live-page | click/fill/read DOM or attach to user-driven page | `references/INTENTS_AUTOMATION.md` |
| login/user-auth | manual auth, MFA, CAPTCHA, real profile follow-up | `references/INTENTS_AUTH.md` |
| emulate/inject/monitor | device/environment emulation, preload patches, long observation | `references/INTENTS_ENVIRONMENT.md` |
| HAR/Playwright/API-replay | export or page network evidence, compare test strategy, mimic discovered APIs | `references/HAR_PLAYWRIGHT_DATA.md` |

## Prefixes
Emit machine-readable lines: `[FINDING]`, `[ACTION]`, `[METRIC]`, `[REASON]`, `[NETWORK_ERROR]`, `[NETWORK_FAILED]`, `[EXCEPTION]`, `[CONSOLE:TYPE]`, `[LOG:LEVEL]`, `[SCREENSHOT]`, `[ARTIFACT]`, `[AUTH_COMPLETE]`, `[AUTH_TIMEOUT]`, `[SOURCEMAP]`.

## Combining Intents
For full audit: run debug/network first, then security/storage/accessibility/performance in separate scripts against the same port. Do not create one huge script unless asked.

Next: load the matching `INTENTS_*.md`; for reusable waits/files/workers load `references/SCRIPT_PATTERNS.md`.
