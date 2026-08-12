# CDP Automation Intents

Load for page actions or live-page attach. Why: prevent accidental navigation/mutation.

## Rules
Enable domains → attach listeners → act. Prefer `--new-tab about:blank` + `Page.navigate` for load evidence. Wait visible/enabled before fill/click (native prototype setter — `dom-operations-check`). One meaningful mutation per iteration; reuse `--port`/`--keep-tab`. Split broad work into small scripts.

## automate
`page-snapshot` → `DOM_REF=…` on `dom-operations-check` (not guessed CSS). Emit `[ACTION]` / `[CODE]`. Multi-step known sequences → one `run(cdp)`; exploratory → per-step sandbox. Confirm with a targeted check, not a full re-snapshot.

## scrape
Read-only counts/samples to files. Broad public crawls → `octocode-scraping` first; CDP validates graph actions and returns URLs/data to that corpus.

## live-page
`--keep-tab`. Listeners miss past events — re-read current state.

## webmcp
Only if the user names WebMCP / page-native AI tools. Fresh Chrome with `--enableFeatures WebMCP` (`chrome-flags.md`); `webmcp-tools.mjs` list/invoke. `[FINDING] WEBMCP_NO_TOOLS` is common → fall back to automate. Mutating tools = mutation gate.

## Mutation gate
Ask before purchases, sends, deletes, account changes, or submitting real user data.

Next: `script-patterns-async.md`; shadow/uploads → `script-patterns-browser.md`.
