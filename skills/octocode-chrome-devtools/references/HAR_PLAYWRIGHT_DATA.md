# HAR, Playwright, And Data-Replay Patterns

Load for HAR export, Playwright comparison, network replay, curl/API mimicry, or token-budget questions.

## Decision Matrix

| Need | Prefer | Why |
|---|---|---|
| live debugging while user interacts | CDP monitor | real console/network/perf events and current DOM |
| deterministic regression test | Playwright HAR | `recordHar` + `routeFromHAR` replay stable traffic |
| inspect a failing API call | CDP Network | request id, timing, failures, optional body on demand |
| mock/replay known API responses | Playwright route/HAR | test-grade routing and assertions |
| understand public site data | CDP then curl/API | discover request, then use documented endpoint |
| huge network capture | HAR + pager | file evidence, small stdout pages |

## HAR Rules

HAR 1.2 is JSON with `log.version`, `creator`, optional `pages`, and `entries`. Each entry carries `startedDateTime`, `time`, `request`, `response`, `cache`, and `timings`. HAR can contain cookies, auth headers, query tokens, and bodies: redact or omit before sharing.

Use stdout only for summary:

```text
[METRIC] requests=128 failed=3 slow=5
[ARTIFACT] HAR .octocode/.../live-network.har
```

Then page the file:

```bash
node <skill-dir>/examples/har-pager.mjs live-network.har --filter failures --page 1 --page-size 25
```

## CDP vs Playwright

Use this skill/CDP for forensics: live page state, authenticated manual sessions, console exceptions, performance, WebSocket/service-worker visibility, and source tracing.

Use Playwright when the goal is a maintained test suite: locators, assertions, retries, cross-browser coverage, fixtures, `recordHar`, `routeFromHAR`, request mocking, and CI stability.

Hybrid path: debug with CDP, write a HAR/summary artifact, then convert the stable flow into Playwright tests or API fixtures.

## Network-To-Curl/API Mimic

When a page displays data:

1. Attach Network before the action.
2. Identify the XHR/fetch endpoint, method, query params, and required headers.
3. Check if a documented API exists; prefer it over scraping DOM.
4. Reproduce with `curl` or `fetch` using only non-secret headers.
5. Page results and write raw data to files.

Never copy cookies, bearer tokens, CSRF values, or private IDs into reports. Say which header names were required, not their values.

## Generic API Replay

When Network evidence reveals a public or approved endpoint, replay only the non-secret request shape:

```bash
curl -s -H "accept: application/json" "https://example.com/api/items?page=1"
```

Agent flow: use browser only if UI behavior matters; otherwise call the endpoint, parse the bounded response, and report stable fields plus pagination/source links when present.

## Token Budget

- Summary stdout: under 2 KB for ordinary runs.
- Raw network/perf/DOM evidence: files under `cdp.outputDir`.
- HAR review: page 10-50 entries at a time.
- DOM review: selector/path/bbox/a11y facts, not `document.body.innerHTML`.
- Response bodies: fetch on demand for the specific request, not all requests.
