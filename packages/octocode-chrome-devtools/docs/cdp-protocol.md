# CDP protocol

Load when a task needs methods beyond a recipe, a new domain, or a different target. Why: choose methods from the running Chrome schema and preserve their evidence.

## Discover before choosing

Run `node "$CDP" protocol`, or call `await cdp.protocol()` in a custom flow. This saves the browser's full `/json/protocol` schema. Page it with `artifact-query.mjs --format json --pointer /domains`; inspect a selected domain, command, event, type, parameter, or return schema by JSON pointer. Follow `$ref` definitions in that same schema. Experimental and deprecated markers describe the schema. Verify method availability on the selected target.

`cdp.send(method, params, sessionId?)` has no domain or method allowlist. The browser decides support. A listed method may require a browser target, an enabled domain, launch flags, or a platform capability. Report protocol errors; change the target or parameters based on the error and schema. Never claim all methods were tested.

## Targets and lifecycle

- Default connection: a page. `--browser`: the browser WebSocket, for browser-level methods. It cannot combine with page selection or stealth.
- In browser mode, use `Target.getTargets`, then `Target.attachToTarget({targetId, flatten:true})`. Send child commands with the returned `sessionId`. Detach when finished.
- For workers and isolated iframes, set `Target.setAutoAttach` before the activity. If `waitForDebuggerOnStart:true` is needed for complete startup evidence, enable domains in the attached session and always call `Runtime.runIfWaitingForDebugger`; otherwise the target remains paused.
- Listen before navigation or action. Use `cdp.on(event, (params, meta) => ...)`; `meta.sessionId` identifies child sessions. `cdp.on('*', (method, params, meta) => ...)` observes all delivered events. Remove listeners with `cdp.off`.
- Enable only the domains the question needs. Debugger pause control depends on the task; a debugging investigation must preserve the requested pauses.
- Install a dialog handler before actions that may open dialogs. Keep dialog actions within existing user authorization.

## Route advanced questions

These are candidate domains, not a fixed supported list. Verify exact methods in the live schema.

| Question | Domains and evidence |
|---|---|
| Rendered state, style, layout, accessibility | DOM, DOMSnapshot, CSS, Accessibility; save complete trees and computed data |
| Input and content readiness | Input, Runtime, Page, DOMDebugger; verify the required visible state and actual events |
| Requests, sockets, interception, offline/cache | Network, Fetch; preserve session IDs, redirects, body completion and WebSocket/EventSource events |
| CPU, rendering, JavaScript, coverage | Tracing, Performance, PerformanceTimeline, Profiler, Debugger; preserve trace streams and source locations |
| Heap, allocations, leaks | HeapProfiler, Memory; save all snapshot chunks and distinguish retained objects from hypotheses |
| Cookies, origin data, workers, caches | Storage, DOMStorage, IndexedDB, CacheStorage, ServiceWorker, Target; inventory before changing state |
| Browser controls, downloads, PDF, streams | Browser, Page, Target, IO; choose the right connection and drain streams to EOF |
| Device, location, network, sensors | Emulation, Network, DeviceOrientation, Sensor when present; record overrides and restore them |
| Security, issues, authentication | Security, Audits, WebAuthn, FedCm when present; keep mutations within authorization |
| Media, animations, layers, overlays | Media, Animation, LayerTree, Overlay; enable listeners before reproducing the issue |
| Protocol domains added by Chrome | The same discovery, session selection, capture and pagination flow |

## Streams and pagination

CDP has different source contracts: native cursors, numbered pages, event chunks, and whole results. Use each method's actual continuation fields; do not invent a universal CDP cursor. Record query parameters, target/session, scope, and any terminal limit. For event sources, record the observation window and attachment gaps.

Drain `IO.read` until `eof`, decode `base64Encoded` chunks, write them in order, and close with `IO.close` in `finally`. For `HeapProfiler.addHeapSnapshotChunk`, append every chunk until the command completes. For live streams, finish the capture before paging it; immutable continuations refuse changed files.

Save whole results with `cdp.saveArtifact`. For large streams, append to a file under `cdp.outputDir`, then page it with `artifact-query.mjs`. Native JSON lists remain available through JSON pointers; text/JSONL uses text pages; PDF, images and arbitrary bytes use base64 pages. Decode each binary page from base64, then concatenate the bytes. Whole-file text/JSON/binary paging uses bounded memory and source-byte offsets. A selected JSON subtree is parsed in memory and uses UTF-16 offsets. Digest checks read the entire source on each page; use selected subtrees or native cursors for focused questions.

Next: task-specific code → `script-patterns.md`; protocol errors → `recovery.md`.
