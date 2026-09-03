# CDP debug intents

Load when diagnosing network, console, DOM, performance, memory, or coverage. Why: pick signals before writing `run(cdp)`.

## debug
Use for one-pass page health. Enable Page, Runtime, Network, Log before navigation. Emit exceptions, console errors, failed requests, status codes, and a short `[FINDING]` summary.

## network
Use for HTTP/API failures. Prefer `network-measure-check.mjs` (`[METRIC] NET health=`). Custom scripts: URL, method, status, type, initiator, timing — omit auth headers/cookies. Deep: `live-har-monitor` / `network-body-har-fetch-check` (`har-capture.md`).

## console
Use for JS/runtime failures. Enable Runtime/Log; emit `[CONSOLE:error]`, `[EXCEPTION]`, and source URL/line when available.

## performance
Use for load metrics. Prefer `scripts/cdp-checks/performance-measure-check.mjs` (FCP/LCP/CLS/long-tasks/resources → `[METRIC] PERF health=`). Enable Performance/Page before navigation in custom scripts; cold load: clear cache first (`references/recovery.md`).

## memory
Use for leak suspicion. Prefer bounded heap samples; avoid long traces unless user asks. Report trend, retained-size clues, and uncertainty.

## dom
Use for selectors, visible text, state, and framework globals. Prefer `Runtime.evaluate` helpers for shadow DOM and app state.

## css-coverage / js-coverage
Use to find unused code. Start coverage before navigation/action, stop after stable state, report percentages, and top unused URLs.

Next: for wait helpers load `references/script-patterns-async.md`; for API enables load `references/cdp-agent.md`.
