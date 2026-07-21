# CDP Automation Intents

Load for page actions, scraping, or live-page attachment. Why: prevent accidental navigation or mutation.

## Smart Automation Rules
1. Lock target, trigger, signals, evidence prefixes before scripting.
2. Enable domains, attach listeners, then act — never reverse that order.
3. Prefer `--new-tab about:blank` + `Page.navigate` inside `run()` for load evidence.
4. Wait for visible/enabled selectors; dispatch framework-visible `input`/`change` events.
5. One meaningful change per iteration; reuse `--port` and `--keep-tab`.
6. Split broad work into small scripts; do not build one giant audit unless asked.
7. On live pages: no navigation unless requested; read current DOM/storage/perf via `Runtime.evaluate`.

## automate
Click/fill/submit only when requested. Record each step as `[ACTION]`.

## scrape
Read structured data without mutation. Emit counts and sample rows; page large output to files.

## live-page
Attach with `--keep-tab`. Listeners miss past events — re-read current state.

## Mutation Gate
Ask before purchases, sends, deletes, account changes, or submitting real user data.

Next: waits in `references/script-patterns-async.md`; shadow DOM/uploads in `references/script-patterns-browser.md`.
