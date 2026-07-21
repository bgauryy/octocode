# CDP Automation Intents

Load when the task requires page actions, scraping, or live-page attachment. Why: prevent accidental navigation or mutation.

## automate
Click/fill/submit only after the user requested the action. Wait for selectors to be visible/enabled, dispatch framework-visible input/change events, and record the exact action as `[ACTION]`.

## scrape
Read structured page data without mutation. Prefer DOM text/attributes via `Runtime.evaluate`; paginate bounded output; emit counts and sample rows, not giant dumps.

## live-page
Attach to a user-driven tab with `--keep-tab`. Do not navigate. Re-read current DOM/storage/performance state because listeners miss past events.

## Mutation Gate
Ask before destructive writes, purchases, sends, deletes, account changes, or submitting real user data.

Next: actionability helpers live in `references/SCRIPT_PATTERNS_ASYNC_WORKERS.md`; file upload and shadow DOM patterns live in `references/SCRIPT_PATTERNS_BROWSER.md`.
