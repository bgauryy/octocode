# CDP protocol

Load when a task needs methods beyond a recipe, a new domain, or a different target. Why: choose methods from the running Chrome schema and preserve their evidence.

## Discover and validate from the installed Chrome

The method list is not declared here: it is the live schema of the Chrome on the connected port (`/json/protocol`, loopback only), cached per browser version under `.octocode/octocode-chrome-devtools/tmp/protocol-cache/` (mode 0600). Reference for the latest protocol: <https://chromedevtools.github.io/devtools-protocol/> (domain pages, e.g. <https://chromedevtools.github.io/devtools-protocol/tot/DOM/>); the installed schema wins when they differ.

- `protocol Domain`: one-line summary of every method and event with experimental/deprecated flags and policy.
- `protocol Domain.member`: compact JSON inline (params with type, optional, enum, description; returns; flags; policy; `docsUrl`). No saved file.
- `protocol --search <keyword>` (or `protocol Domain --search <keyword>`): ranked hits over names, parameter names and descriptions, 25 per page; `next.continue` runs the following page (`--offset`).
- `protocol` alone saves the full schema; page it with `artifact --pointer /domains`. Follow `$ref` types in that schema.

Every `cdp` call (tool, CLI, plan op) is checked before it is sent: policy first, then the schema. Invalid params, a missing required param, a wrong type, an enum miss or an unknown parameter are refused locally with the expected params, the docs link and, for a mistyped method, the nearest names. `$step` references are checked once resolved at run time. Experimental and deprecated methods print a `[WARNING]` line. If the schema cannot be read, the call is sent unvalidated with a `[NOTE]`. A listed method may still need a target, an enabled domain, launch flags or a platform capability: report the protocol error and adjust from it. Never claim all methods were tested.

## Safety policy

One table (`src/engine/cdp-policy.mts`) classifies methods; every unlisted method is free.

**Blocked**: refused always, never sent.

| Method | Why and instead |
|---|---|
| `Browser.close` | closes the whole browser and every tab in it. Use the cleanup command to close a browser the engine opened. |
| `Browser.crash` | deliberately crashes the browser process. Use the cleanup command to close a browser the engine opened. |
| `Browser.crashGpuProcess` | deliberately crashes the GPU process. Use the cleanup command to close a browser the engine opened. |
| `Page.crash` | deliberately crashes the page process. Use goto or reload to reset a page. |
| `Page.close` | closes the page without releasing engine state. Use --close-tab, or the cleanup command for a browser the engine opened. |
| `Security.setIgnoreCertificateErrors` | turns off certificate validation for every page. Fix the certificate, or open the browser with the documented launch option. |
| `Target.setRemoteLocations` | exposes the browser to remote debugging locations. Connect to the loopback port only. |
| `Target.exposeDevToolsProtocol` | hands the DevTools protocol to page scripts. Send the method from the engine instead. |

**Needs the user's permission**: refused unless the call carries `acknowledge: true` (CLI `--acknowledge`, plan step field `acknowledge`). Ask the user first; the flag records that they agreed.

| Method | Why |
|---|---|
| `Browser.grantPermissions` | grants site permissions (camera, location, clipboard...) |
| `Browser.setPermission` | changes a site permission |
| `Browser.resetPermissions` | resets every site permission |
| `Browser.setDownloadBehavior` | lets pages write files to disk |
| `Network.setCookie` | writes a cookie into the profile |
| `Network.setCookies` | writes cookies into the profile |
| `Network.deleteCookies` | deletes cookies from the profile |
| `Network.clearBrowserCookies` | deletes every cookie |
| `Network.getAllCookies` | reads every cookie value, secrets included |
| `Network.getCookies` | reads cookie values, secrets included |
| `Network.setExtraHTTPHeaders` | adds headers to every request |
| `Network.setRequestInterception` | intercepts and can alter requests |
| `Storage.getCookies` | reads cookie values, secrets included |
| `Storage.setCookies` | writes cookies into the profile |
| `Storage.clearCookies` | deletes cookies from the profile |
| `Storage.clearDataForOrigin` | erases stored site data |
| `DOMStorage.setDOMStorageItem` | writes site storage |
| `IndexedDB.clearObjectStore` | erases stored site data |
| `IndexedDB.deleteDatabase` | erases a site database |
| `CacheStorage.deleteCache` | erases a site cache |
| `Page.setBypassCSP` | disables the page content security policy |
| `Page.addScriptToEvaluateOnNewDocument` | runs a script in every later document of the page |
| `Emulation.setGeolocationOverride` | changes the location sites see |
| `WebAuthn.addVirtualAuthenticator` | adds a fake authenticator |
| `WebAuthn.addCredential` | stores a credential |
| `WebAuthn.getCredentials` | reads stored credentials |
| `Fetch.fulfillRequest` | answers a request with a made-up reply |
| `Fetch.continueRequest` | modifies traffic in flight |
| `Fetch.failRequest` | drops traffic in flight |

A plan is refused before any browser call when a literal method is blocked, or needs permission and has no `acknowledge`. Runner scripts (`script`) are raw by design: `cdp.send` refuses only the blocked methods; needsAck is not enforced there.

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

Save whole results with `cdp.saveArtifact`. For large streams, append to a file under `cdp.outputDir`, then page it with `octocode-chrome-devtools /raw artifact --file <path>`. Native JSON lists remain available through JSON pointers; text/JSONL uses text pages; PDF, images and arbitrary bytes use base64 pages. Decode each binary page from base64, then concatenate the bytes. Whole-file text/JSON/binary paging uses bounded memory and source-byte offsets. A selected JSON subtree is parsed in memory and uses UTF-16 offsets. Digest checks read the entire source on each page; use selected subtrees or native cursors for focused questions.

Next: task-specific code → `script-patterns.md`; protocol errors → `recovery.md`.
