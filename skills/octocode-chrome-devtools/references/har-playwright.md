# HAR, Playwright, And Data Replay

Load for HAR export, Playwright comparison, API replay, or token-budget questions. Why: keep evidence in files and secrets out of chat.

## Decision Matrix
| Need | Prefer | Why |
|---|---|---|
| live debug while user acts | CDP monitor | real console/network/DOM |
| CI regression | Playwright HAR | `recordHar` / `routeFromHAR` |
| failing API forensics | CDP Network | status, timing, initiator |
| mock known responses | Playwright routes | assertions + retries |
| public data | CDP then curl/API | discover, then documented endpoint |
| huge capture | HAR + pager + redact | small stdout pages |

## HAR Rules
Write HAR under `cdp.outputDir`. Stdout: counts + `[ARTIFACT]` path only. Page with `examples/har-pager.mjs`. Before sharing, run `examples/har-redact.mjs` (cookies/auth headers/query secrets → `[REDACTED]`). Live monitor already omits cookie/auth header values.

```bash
node <skill-dir>/examples/har-pager.mjs live-network.har --filter failures --page 1
node <skill-dir>/examples/har-redact.mjs live-network.har --strip-bodies
```

## Fetch One Response Body By requestId
`examples/live-har-monitor.mjs` captures request/timing/status only — HAR entries always have empty `response.content.text`, by design, to stay fast and small. When you need the actual body for one specific request, write a focused script instead of relying on the HAR:

```js
// Inside a run(cdp) script, with Network already enabled and the requestId
// captured from Network.responseReceived/requestWillBeSent:
const { body, base64Encoded } = await cdp.send('Network.getResponseBody', { requestId });
const text = base64Encoded ? Buffer.from(body, 'base64').toString('utf8') : body;
```

Call this while the request is still cached — Chrome evicts bodies after navigation/reload. If it returns nothing, see `references/recovery.md`.

## Hybrid
Debug with CDP → save HAR/summary → promote stable flows to Playwright or API fixtures. Never copy cookie/bearer/CSRF values into reports — header names only.

## Token Budget
Summary <2KB; raw evidence in files; page HAR 10–50 rows; fetch bodies on demand for one requestId.

Next: live monitor in `examples/README.md`; recovery in `references/recovery.md`.
