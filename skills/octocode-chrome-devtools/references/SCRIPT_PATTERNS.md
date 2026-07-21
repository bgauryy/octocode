# Script Pattern Router

Load when a CDP script needs reusable helper shape. Why: choose a focused pattern instead of copying a giant script.

## Loading Rule
Start from the intent detail file, then load at most one pattern detail file.

## Pattern Routes
| Need | Detail |
|---|---|
| network idle, selector actionability, service workers, worker websockets | `references/SCRIPT_PATTERNS_ASYNC_WORKERS.md` |
| websockets, resource search, file upload, screenshots/PDFs, shadow DOM, source maps | `references/SCRIPT_PATTERNS_BROWSER.md` |
| network/console observation, performance, web vitals, DOM/a11y, heap, security | `references/SCRIPT_PATTERNS_OBSERVE.md` |
| storage, consent, full-audit composition | `references/SCRIPT_PATTERNS_SPECIAL.md` |
| HAR export, Playwright comparison, API/curl replay, token budgets | `references/HAR_PLAYWRIGHT_DATA.md` |

## Output Rule
Write files only under `cdp.outputDir`; emit paths with a prefix such as `[SCREENSHOT]`, `[SOURCEMAP]`, or `[ARTIFACT]`.

Next: load exactly one matching detail file, then write `.octocode/tmp/cdp-<task>.mjs`.
