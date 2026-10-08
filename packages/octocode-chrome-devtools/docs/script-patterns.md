# Custom script patterns

Load when you write a task-specific `run(cdp)` or combine recipes. Why: adapt methods, sessions, readiness and evidence to the question. The template exports `async function run(cdp)`. The runner provides `cdp.send(method, params, sessionId?)`, `cdp.on(event, fn)`, `cdp.targetInfo`, and `cdp.outputDir`. Write files only under `cdp.outputDir` and print paths with `[ARTIFACT]`/`[SCREENSHOT]`; other prefixes: `[FINDING]` `[ACTION]` `[CODE]` `[METRIC]` `[REASON]` `[EXCEPTION]` `[CONSOLE:TYPE]` `[NETWORK_FAILED]` `[SOURCEMAP]`. The sandbox blocks `child_process` and Node workers. Global fetch/WebSocket is localhost-only; Node core networking remains available. Import staged helpers by cwd path: `await import(pathToFileURL(resolve(process.cwd(), '.octocode', 'human-input.mjs')).href)`.

## Choose the flow

Read the live schema with `await cdp.protocol()`. Choose the target and methods that answer the question, attach listeners, act, wait for the required condition, verify, save full evidence, then clean up. Use a recipe only when its behavior and observation window fit. Custom scripts can use any schema method through `cdp.send`.

Use `cdp.saveArtifact('capture.json', result)` for JSON, or pass `text`/`binary` as its third argument. It refuses overwrites and prints a lossless paging command. For streamed captures, append every chunk to a file under `cdp.outputDir`, wait for completion, then emit `artifact-query.mjs --file <path> --format text|binary`. Save console, exceptions, raw events, DOM, storage and trace data the same way; summaries are indexes into that evidence.

## Timing

- Attach listeners before acting. `--new-tab <url>` loads before `run` starts, so for load evidence `Page.navigate` (or `Page.reload`, as the template does) after attaching listeners.
- **Content ready**: wait for the specific selector, text, or application state needed by the next action. Document `interactive` only establishes a usable document; it does not prove a SPA has finished rendering. Network idle is a separate signal, and polling or background traffic may prevent it. Always bound waits and report the unmet condition.
- **Network idle**: track in-flight requests by session and request ID, resolve after a quiet window, skip websockets and long polling, always time out.
- **Wait for element**: poll `Runtime.evaluate` for existence, non-zero size, `el.matches(':disabled')` (covers disabled `<fieldset>`), and a stable box.
- **Input**: in custom scripts use `human-input.mjs`: `runEventSequence(cdp, buildElementClickSequence(x, y, rect, true))`, `buildTypingEvents(text)` (real keydown/keyup), `buildKeyPressEvents('Control+a')`. Synthetic fallback: the prototype's native `value` setter plus `input`/`change`.

## Observe an action and its requests together

Import the canonical modules using their absolute file URLs in a custom runner script:

```js
import { run as monitor } from '/absolute/package/dist/engine/cdp-checks/live-har-monitor.mjs';
import { run as act } from '/absolute/package/dist/engine/cdp-checks/dom-operations-check.mjs';
export async function run(cdp) {
  await monitor(cdp, { onReady: () => act(cdp) });
}
```

Set `DOM_ROLE`, `DOM_NAME`, `DOM_ACTION`, the expected `DOM_WAIT_TEXT`, and `DOM_TRACE_EVENTS=1`. The monitor installs listeners before invoking the action, then observes for `MONITOR_MS`. Event traces include epoch timestamps and `isTrusted`, so they can be aligned with HAR request times. Traces omit typed characters; captured requests can still contain sensitive data.

Waits after an action search the target's document or shadow root, including accessible child frames. For isolated cross-origin frames, select the reported frame target and capture its own refs. Related iframe network sessions are attached automatically; coverage explicitly reports attachment errors and possible missed initial requests.

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
