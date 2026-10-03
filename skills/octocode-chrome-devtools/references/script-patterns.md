# Custom script patterns

Load when no ready check fits and you write `run(cdp)`. The template exports `async function run(cdp)`. The runner provides `cdp.send(method, params, sessionId?)`, `cdp.on(event, fn)`, `cdp.targetInfo`, and `cdp.outputDir`. Write files only under `cdp.outputDir` and print paths with `[ARTIFACT]`/`[SCREENSHOT]`; other prefixes: `[FINDING]` `[ACTION]` `[CODE]` `[METRIC]` `[REASON]` `[EXCEPTION]` `[CONSOLE:TYPE]` `[NETWORK_FAILED]` `[SOURCEMAP]`. The sandbox blocks `child_process`, workers, and non-CDP network. Import staged helpers by cwd path: `await import(pathToFileURL(resolve(process.cwd(), '.octocode', 'human-input.mjs')).href)`.

## Timing

- Attach listeners before acting. `--new-tab <url>` loads before `run` starts, so for load evidence `Page.navigate` (or `Page.reload`, as the template does) after attaching listeners.
- **Network idle**: track in-flight requests, resolve after a quiet window, skip websockets and long polling, always time out.
- **Wait for element**: poll `Runtime.evaluate` for existence, non-zero size, `el.matches(':disabled')` (covers disabled `<fieldset>`), and a stable box.
- **Input**: in custom scripts use `human-input.mjs`: `runEventSequence(cdp, buildElementClickSequence(x, y, rect, true))`, `buildTypingEvents(text)` (real keydown/keyup), `buildKeyPressEvents('Control+a')`. Synthetic fallback: the prototype's native `value` setter plus `input`/`change`.

## Browser surfaces

- **Shadow DOM**: `DOM.querySelector` doesn't pierce; walk shadow roots in `Runtime.evaluate` and return paths, not DOM dumps.
- **Upload**: `DOM.setFileInputFiles` with absolute paths, then `change`.
- **Workers / service workers**: Target auto-attach; keep `{targetId, sessionId, url, role}` per target.
- **WebSocket**: Network events `webSocketCreated`/`FrameSent`/`FrameReceived`/`Closed`.
- **Source maps**: `createSourceMapResolver(cdp)` before navigation, `await settle()`, then `resolve(scriptId, line, col)`; `printSummary()` emits `[SOURCEMAP]`. It is the one helper allowed real outbound HTTP(S).
- **Event listeners**: `DOMDebugger.getEventListeners({objectId})` maps handlers on vanilla sites; frameworks show one delegated root listener.

## Observation

- **Web vitals**: inject PerformanceObserver before navigation; report missing support as uncertainty.
- **Heap**: `HeapProfiler.takeHeapSnapshot` with `addHeapSnapshotChunk`.
- **Security**: `Security`/`Audits` listeners before navigation.

Next: domain order and methods → `cdp-protocol.md`; failures → `recovery.md`.
