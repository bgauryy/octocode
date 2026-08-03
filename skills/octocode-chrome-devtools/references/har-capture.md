# HAR Capture And Data Replay

Load for HAR export, API replay, or token-budget questions. Why: keep evidence in files and secrets out of chat.

## Decision Matrix
| Need | Use | Why |
|---|---|---|
| live debug while user acts | CDP monitor | real console/network/DOM |
| failing API forensics | CDP Network | status, timing, initiator |
| public data | CDP then curl/API | discover, then documented endpoint |
| huge capture | HAR + pager + redact | small stdout pages |

## HAR Rules
Write HAR under `cdp.outputDir`. Stdout: counts + `[ARTIFACT]` path only. Page with `examples/har-pager.mjs`. Before sharing, run `examples/har-redact.mjs` (cookies/auth headers/query secrets → `[REDACTED]`). Live monitor already omits cookie/auth header values.

```bash
node <skill-dir>/examples/har-pager.mjs live-network.har --filter failures --page 1
node <skill-dir>/examples/har-redact.mjs live-network.har --strip-bodies
```

## Scope: HTTP(S) Only
A single top-level `Network.enable` captures requests from every frame/iframe on the page, not just the main document — no per-frame instrumentation needed. It does not capture WebSocket frames: HAR is an HTTP-archive format with no WS section, and `examples/live-har-monitor.mjs` does not listen for `Network.webSocket*` events. For WebSocket traffic use the `websocket` intent in `references/intents-inspect.md` instead (`Network.webSocketCreated`/`webSocketFrameSent`/`webSocketFrameReceived`).

## Fetch One Response Body By requestId
`examples/live-har-monitor.mjs` captures request/timing/status only — HAR entries always have empty `response.content.text`, by design, to stay fast and small. When you need the actual body for one specific request, write a focused script instead of relying on the HAR:

```js
// Inside a run(cdp) script, with Network already enabled and the requestId
// captured from Network.responseReceived/requestWillBeSent:
const { body, base64Encoded } = await cdp.send('Network.getResponseBody', { requestId });
const text = base64Encoded ? Buffer.from(body, 'base64').toString('utf8') : body;
```

Call this while the request is still cached — Chrome evicts bodies after navigation/reload. If it returns nothing, see `references/recovery.md`.

For interception (mock a response, or capture a body alongside a fresh HAR/artifact set) rather than a passive one-off fetch, use `examples/network-body-har-fetch-check.mjs` — it already wires `Fetch.enable` before navigation to `Network.getResponseBody` and writes both HAR and body artifacts.

## Hybrid
Debug with CDP → save HAR/summary → promote stable flows to a maintained test suite or API fixtures. Never copy cookie/bearer/CSRF values into reports — header names only.

## Token Budget
Summary <2KB; raw evidence in files; page HAR 10–50 rows; fetch bodies on demand for one requestId. Before a new browser check, search saved artifacts under `.octocode/tmp/chrome-devtools/` with local search tools (`localSearchCode`/`localGetFileContent`) — skip the CDP round-trip if the answer is already on disk. Run `scripts/prune-artifacts.mjs` periodically; these directories are never deleted automatically.

Next: live monitor in `examples/README.md`; recovery in `references/recovery.md`.
