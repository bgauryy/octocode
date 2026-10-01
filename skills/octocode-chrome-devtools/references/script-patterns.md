# Custom script patterns

Load when no ready check fits and you write `run(cdp)`. Start from `scripts/cdp-template.mjs` saved as `.octocode/tmp/cdp-<task>.mjs`, exporting `async function run(cdp)`. The runner provides `cdp.send(method, params, sessionId?)`, `cdp.on(event, fn)`, `cdp.targetInfo`, and `cdp.outputDir`. Write files only under `cdp.outputDir` and print paths with `[ARTIFACT]`/`[SCREENSHOT]`; other prefixes: `[FINDING]` `[ACTION]` `[CODE]` `[METRIC]` `[REASON]` `[EXCEPTION]` `[CONSOLE:TYPE]` `[NETWORK_FAILED]` `[SOURCEMAP]`. The sandbox blocks `child_process`, workers, and non-CDP network. Import staged helpers by cwd path: `await import(pathToFileURL(resolve(process.cwd(), '.octocode', 'human-input.mjs')).href)`.

## Timing

- Enable domains and attach listeners before acting. `--new-tab <url>` loads before `run` starts, so for load evidence `Page.navigate` (or `Page.reload`, as the template does) after attaching listeners.
- **Network idle**: track in-flight requests, resolve after a quiet window, skip websockets and long polling, always time out.
- **Wait for element**: poll `Runtime.evaluate` for existence, non-zero size, `el.matches(':disabled')` (covers disabled `<fieldset>`), and a stable box.
- **Input**: prefer `dom-operations-check`. In custom scripts use `human-input.mjs`: `runEventSequence(cdp, buildElementClickSequence(x, y, rect, true))`, `buildTypingEvents(text)` (real keydown/keyup), `buildKeyPressEvents('Control+a')`. Synthetic fallback: the prototype's native `value` setter plus `input`/`change`.

## Browser surfaces

- **Shadow DOM**: `DOM.querySelector` doesn't pierce; walk shadow roots in `Runtime.evaluate` and return paths, not DOM dumps.
- **Upload**: `DOM.setFileInputFiles` with absolute paths, then `change`. Ask before uploading sensitive files.
- **Workers / service workers**: Target auto-attach; keep `{targetId, sessionId, url, role}` and pass `sessionId` to `send`.
- **WebSocket**: Network events `webSocketCreated`/`FrameSent`/`FrameReceived`/`Closed`; counts and redacted samples.
- **Source maps**: `createSourceMapResolver(cdp)` before navigation, `await settle()`, then `resolve(scriptId, line, col)`; `printSummary()` emits `[SOURCEMAP]`. It is the one helper allowed real outbound HTTP(S).
- **Event listeners**: `DOMDebugger.getEventListeners({objectId})` maps handlers on vanilla sites; frameworks show one delegated root listener.

## Observation

- **Console/network**: Network + Runtime + Log; failed requests, error statuses, exceptions with location.
- **Web vitals**: inject PerformanceObserver before navigation; report missing support as uncertainty.
- **Heap**: `HeapProfiler.takeHeapSnapshot` with `addHeapSnapshotChunk`, before/after one action.
- **Security**: `Security`/`Audits` listeners before navigation.

## Compound audits

Run debug/network, security, storage, a11y, performance, and screenshots as separate small scripts on one port; merge findings in the answer.

Next: domain order and methods → `cdp-protocol.md`; failures → `recovery.md`.
