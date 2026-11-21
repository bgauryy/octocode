# CDP protocol

Load when a task needs methods beyond a recipe, a new domain, or a different target. Why: choose methods from the running Chrome schema and preserve their evidence.

## Discover and validate from the installed Chrome

The method list is not declared here: it is the live schema of the Chrome on the connected port (`/json/protocol`, loopback only), cached per browser version under `.octocode/octocode-chrome-devtools/tmp/protocol-cache/` (mode 0600). Reference for the latest protocol: <https://chromedevtools.github.io/devtools-protocol/> (domain pages, e.g. <https://chromedevtools.github.io/devtools-protocol/tot/DOM/>); the installed schema wins when they differ.

- `protocol *`: one compact row per domain `{name, commands, events, needsEnable, dependencies, experimental}`, 25 per page; `next.continue` runs the following page (`--offset`). Nothing is capped silently.
- `protocol Domain`: one-line summary of every method and event with experimental/deprecated flags and policy, plus `enable {available, params, dependencies, events}` (what `<Domain>.enable` is in this Chrome), `startsEmitting` (the events enable turns on), `playbook` when the domain has one (`{summary, steps: ["Domain.method {key params}", ...], notes?}`: the typical call order from `src/engine/cdp-playbooks.json`, about 20 domains), `docsUrl` and `docsHash`.
- `protocol Domain.member`: compact JSON inline (params with type, optional, enum, description; returns; flags; policy; `docsUrl`). No saved file.
- `protocol --search <keyword>` (or `protocol Domain --search <keyword>`): ranked hits over names, parameter names and descriptions, 25 per page; `next.continue` runs the following page (`--offset`).
- `protocol` alone saves the full schema; page it with `artifact --pointer /domains`. Follow `$ref` types in that schema.

Every `cdp` call (tool, CLI, plan op) is checked before it is sent: policy first, then the schema. Validation refuses only clear errors: a missing required param, a wrong primitive type, an enum value outside the list, a wrong array/object shape, or an unknown method (with the expected params, the docs link and the nearest names). An unknown parameter name is a `[WARNING]` and is sent: a newer Chrome adds parameters and Chrome rejects a real unknown itself. Anything uncertain goes to Chrome. `$step` references are checked once resolved at run time. Experimental and deprecated methods print a `[WARNING]` line. If the schema cannot be read, the call is sent unvalidated with a `[NOTE]`. A listed method may still need a target, launch flags or a platform capability: report the protocol error and adjust from it. Never claim all methods were tested.

## Where a call runs (`scope`)

Every Chrome method lives on one connection: about 530 on a page session, about 30 on the browser connection only (`SystemInfo.*`, `Tethering.*`, `BluetoothEmulation.*`, `Target.createBrowserContext`/`getBrowserContexts`/`disposeBrowserContext`...), about 25 on both. `cdp` takes `scope`:

| `scope` | Behavior |
|---|---|
| `auto` (default) | page session first; on Chrome's wrong-scope answer retry once on the browser connection (opened lazily from `/json/version` of the same port). The step reports `scope: "browser"` when it ran there. |
| `page` | the page session only; Chrome's error is returned as is. |
| `browser` | the browser connection only. |

Wrong-scope answers are the non-executing ones: `'X.m' wasn't found` (-32601, and the quoted name equals the method sent), `Not allowed` (-32000) and `... is only supported on the Browser target` (-32000). A -32601 that names another method is a failure Chrome forwarded and is returned as is. If both connections answer -32601 the call fails with `[UNSUPPORTED]`: this Chrome (a headless or custom build) does not implement the method; the message links its docs page. With `--browser` the attached target already is the browser: there is no second scope, and page-only methods need a page target. Hangs and crashes (`Page.waitForDebugger`, `Input.synthesizePinchGesture`, `Emulation.setDisplayFeaturesOverride`...) are left to the per-command deadline; one call never stalls the plan.

## Enabling domains

Many methods need their domain enabled first. Nothing needs a hand-written list:

- `enable: ["CSS", ...]` on a `cdp` call (CLI `--enable CSS,DOM`) and `enable` on the plan (runs before step 1): each name is validated against the installed Chrome (unknown names are refused with the nearest names), ordered so a domain follows the requested domains it depends on, and `<Domain>.enable` is sent without params. The step reports `enabled: [...]` in the order sent. Domains without an `enable` command, or whose `enable` has required parameters (`BluetoothEmulation`, `PerformanceTimeline`), are skipped with a note: send their `enable` yourself.
- Automatic: when Chrome answers a call with "`Accessibility has not been enabled`", "`Log is not enabled`", "`X agent was not enabled`", "`X domain is not enabled`", "`DOM agent needs to be enabled first`", "`Overlay must be enabled before`" and the like, the domain is enabled and the call retried once. A wording that names no domain (`Log is not enabled`) means the method's own domain; one that names another (`DOM agent needs to be enabled first`) means that one; `<Domain>.enable` and `<Domain>.disable` never trigger it. When the retry is impossible (`Fetch`, or an `enable` that needs parameters) or the domain is still not enabled, the error ends with `hint: enable the domain in the same call: enable:["X"] (or run it inside a plan)` (an enable that that itself needs another domain, such as `CSS.enable` needing `DOM`, enables that first). It never loops. The schema's `dependencies` are type dependencies, not enable requirements, so unrequested dependencies are enabled only when Chrome asks for them.
- `listen` and `waitEvent` enable the domain of the event they watch.
- Each connection remembers what was enabled: nothing is enabled twice in a plan. Enabled state ends with the connection, so a one-call `cdp` cannot leave a domain enabled for the next call: use a plan.
- Override methods are per connection too (`Emulation.set*`, `Network.setExtraHTTPHeaders`/`setCacheDisabled`/`setUserAgentOverride`/`setBlockedURLs`/`emulateNetworkConditions`, `Page.addScriptToEvaluateOnNewDocument`/`setLifecycleEventsEnabled`/`setBypassCSP`, `Fetch.enable`, `Browser.grantPermissions`/`setPermission`; the list is `CONNECTION_OVERRIDE_METHODS` in `src/engine/cdp-policy.mts`). When a run's last step is one of them the reply `warnings` carry `[NOTE] <method> applies only to this connection; keep the dependent steps in the same run, or persist with open emulation`. Put the dependent steps in the same plan: that does not warn.
- `Fetch` is never enabled automatically: its `enable` pauses requests without patterns. Call `Fetch.enable` yourself with the patterns.

## Safety policy

One table (`src/engine/cdp-policy.mts`) classifies methods; every unlisted method is free. There is one tier: a method that needs the user's permission is refused unless the call carries `acknowledge: true`, and the refusal names its consequence.

**Needs the user's permission**: refused unless the call carries `acknowledge: true` (CLI `--acknowledge`, plan step field `acknowledge`). Ask the user first; the flag records that they agreed.

| Method | Why |
|---|---|
| `Browser.close` | closes the whole browser and every tab in it (prefer cleanup for a browser this package launched) |
| `Browser.crash` | deliberately crashes the browser process and every tab |
| `Browser.crashGpuProcess` | deliberately crashes the GPU process, which can blank every tab |
| `Page.crash` | deliberately crashes the page process and loses its state (use goto or reload to reset a page) |
| `Page.close` | closes the page without releasing engine state (prefer --close-tab, or cleanup for a browser this package launched) |
| `Security.setIgnoreCertificateErrors` | turns off certificate validation for every page (prefer fixing the certificate) |
| `Target.setRemoteLocations` | exposes the browser to remote debugging locations (prefer loopback only) |
| `Target.exposeDevToolsProtocol` | hands the DevTools protocol to page scripts |
| `Browser.grantPermissions` | grants site permissions (camera, location, clipboard...) |
| `Runtime.evaluate` | runs arbitrary JavaScript in the page; for reading state use the evaluate plan op |
| `Runtime.callFunctionOn` | runs arbitrary JavaScript in the page; for reading state use the evaluate plan op |
| `Runtime.compileScript` | compiles arbitrary JavaScript in the page; for reading state use the evaluate plan op |
| `Runtime.runScript` | runs arbitrary JavaScript in the page; for reading state use the evaluate plan op |
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
| `Page.setGeolocationOverride` | deprecated twin of `Emulation.setGeolocationOverride` |
| `Page.deleteCookie` | deprecated twin of `Network.deleteCookies` |
| `Page.setDownloadBehavior` | deprecated twin of `Browser.setDownloadBehavior` |
| `Page.addScriptToEvaluateOnLoad` | deprecated twin of `Page.addScriptToEvaluateOnNewDocument` |
| `Security.setOverrideCertificateErrors` | changes how certificate errors are handled for every page |
| `Security.handleCertificateError` | continues past a certificate error |
| `WebAuthn.addVirtualAuthenticator` | adds a fake authenticator |
| `WebAuthn.addCredential` | stores a credential |
| `WebAuthn.getCredentials` | reads stored credentials |
| `Fetch.fulfillRequest` | answers a request with a made-up reply |
| `Fetch.continueRequest` | modifies traffic in flight |
| `Fetch.failRequest` | drops traffic in flight |
| `Storage.getTrustTokens` | reads Private State Token issuers and counts the browser holds |
| `Storage.getPrivateVerificationTokens` | reads the private verification tokens the browser holds |
| `Storage.overrideQuotaForOrigin` | changes the storage quota a site sees |
| `Storage.deleteStorageBucket` | erases a storage bucket (known to crash some Chrome builds) |
| `Storage.clearTrustTokens` | erases Private State Tokens |
| `Storage.clearDataForStorageKey` | erases stored site data |
| `IndexedDB.requestData` | reads stored site records, secrets included |
| `IndexedDB.getMetadata` | reads site database metadata |
| `IndexedDB.deleteObjectStoreEntries` | erases records of a site database |
| `CacheStorage.requestCachedResponse` | reads a cached response body |
| `CacheStorage.requestEntries` | lists the entries of a site cache |
| `CacheStorage.deleteEntry` | erases an entry of a site cache |
| `DOMStorage.getDOMStorageItems` | reads site storage values, secrets included |
| `DOMStorage.removeDOMStorageItem` | erases a site storage item |
| `DOMStorage.clear` | erases a site storage |
| `Input.dispatchKeyEvent` | sends raw input to the page without the act operation actionability checks (prefer act) |
| `Input.dispatchMouseEvent` | sends raw input to the page without the act operation actionability checks (prefer act) |
| `Input.dispatchTouchEvent` | sends raw input to the page without the act operation actionability checks (prefer act) |
| `Input.dispatchDragEvent` | sends raw input to the page without the act operation actionability checks (prefer act) |
| `Input.insertText` | sends raw input to the page without the act operation actionability checks (prefer act) |
| `Input.imeSetComposition` | sends raw input to the page without the act operation actionability checks (prefer act) |
| `Input.synthesizePinchGesture` | sends raw input to the page without the act operation actionability checks (prefer act) |
| `Input.synthesizeScrollGesture` | sends raw input to the page without the act operation actionability checks (prefer act) |
| `Input.synthesizeTapGesture` | sends raw input to the page without the act operation actionability checks (prefer act) |
| `Input.setIgnoreInputEvents` | sends raw input to the page without the act operation actionability checks (prefer act) |
| `Input.emulateTouchFromMouseEvent` | sends raw input to the page without the act operation actionability checks (prefer act) |
| `Autofill.trigger` | fills a form with stored autofill data |
| `Autofill.setAddresses` | replaces the autofill addresses the browser holds |
| `Extensions.loadUnpacked` | installs an extension from disk |
| `Extensions.uninstall` | removes an extension |
| `Extensions.clearStorageItems` | changes extension storage |
| `Extensions.setStorageItems` | changes extension storage |
| `PWA.install` | installs a web app on the system |
| `PWA.launch` | launches an installed web app |
| `PWA.uninstall` | uninstalls a web app |
| `FedCm.selectAccount` | signs in through the federated login dialog |
| `FedCm.dismissDialog` | dismisses the federated login dialog |
| `FedCm.clickDialogButton` | clicks a federated login dialog button |
| `FedCm.openUrl` | opens a link from the federated login dialog |
| `DigitalCredentials.setVirtualWalletBehavior` | installs a virtual digital wallet that answers credential requests |
| `WebAuthn.removeCredential` | removes stored credentials |
| `WebAuthn.clearCredentials` | removes stored credentials |
| `BluetoothEmulation.enable` | drives virtual Bluetooth hardware the page can see |
| `BluetoothEmulation.setSimulatedCentralState` | drives virtual Bluetooth hardware the page can see |
| `BluetoothEmulation.disable` | drives virtual Bluetooth hardware the page can see |
| `BluetoothEmulation.simulatePreconnectedPeripheral` | drives virtual Bluetooth hardware the page can see |
| `BluetoothEmulation.simulateAdvertisement` | drives virtual Bluetooth hardware the page can see |
| `BluetoothEmulation.simulateGATTOperationResponse` | drives virtual Bluetooth hardware the page can see |
| `BluetoothEmulation.simulateCharacteristicOperationResponse` | drives virtual Bluetooth hardware the page can see |
| `BluetoothEmulation.simulateDescriptorOperationResponse` | drives virtual Bluetooth hardware the page can see |
| `BluetoothEmulation.addService` | drives virtual Bluetooth hardware the page can see |
| `BluetoothEmulation.removeService` | drives virtual Bluetooth hardware the page can see |
| `BluetoothEmulation.addCharacteristic` | drives virtual Bluetooth hardware the page can see |
| `BluetoothEmulation.removeCharacteristic` | drives virtual Bluetooth hardware the page can see |
| `BluetoothEmulation.addDescriptor` | drives virtual Bluetooth hardware the page can see |
| `BluetoothEmulation.removeDescriptor` | drives virtual Bluetooth hardware the page can see |
| `BluetoothEmulation.simulateGATTDisconnection` | drives virtual Bluetooth hardware the page can see |
| `SmartCardEmulation.enable` | drives a virtual smart card reader the page can see |
| `SmartCardEmulation.disable` | drives a virtual smart card reader the page can see |
| `SmartCardEmulation.reportEstablishContextResult` | drives a virtual smart card reader the page can see |
| `SmartCardEmulation.reportReleaseContextResult` | drives a virtual smart card reader the page can see |
| `SmartCardEmulation.reportListReadersResult` | drives a virtual smart card reader the page can see |
| `SmartCardEmulation.reportGetStatusChangeResult` | drives a virtual smart card reader the page can see |
| `SmartCardEmulation.reportBeginTransactionResult` | drives a virtual smart card reader the page can see |
| `SmartCardEmulation.reportPlainResult` | drives a virtual smart card reader the page can see |
| `SmartCardEmulation.reportConnectResult` | drives a virtual smart card reader the page can see |
| `SmartCardEmulation.reportDataResult` | drives a virtual smart card reader the page can see |
| `SmartCardEmulation.reportStatusResult` | drives a virtual smart card reader the page can see |
| `SmartCardEmulation.reportError` | drives a virtual smart card reader the page can see |
| `Fetch.continueWithAuth` | answers an authentication challenge in flight |
| `Fetch.getResponseBody` | reads a paused response body |
| `Network.getResponseBody` | reads a response body, secrets included |
| `Network.loadNetworkResource` | fetches a URL through the browser with its credentials |
| `Network.setCacheDisabled` | changes caching for every request |
| `Network.emulateNetworkConditions` | changes the network conditions of every request |
| `Network.overrideNetworkState` | changes the network conditions of every request |
| `Network.replayXHR` | re-sends a request, side effects included |
| `Network.setBlockedURLs` | blocks matching requests for every page |
| `Debugger.setScriptSource` | rewrites running script source |
| `Debugger.setVariableValue` | changes a variable of a paused frame |
| `Debugger.evaluateOnCallFrame` | runs arbitrary JavaScript in a paused frame |
| `Debugger.setReturnValue` | changes the value a paused function returns |
| `Page.setInterceptFileChooserDialog` | takes over the page file chooser |
| `Page.setWebLifecycleState` | freezes or discards the page |
| `Page.navigateToHistoryEntry` | moves the page through its history |
| `Page.generateTestReport` | sends a report to the page reporting endpoint |
| `Page.setDocumentContent` | replaces the page document |
| `Emulation.setUserAgentOverride` | changes the user agent every site sees |
| `Emulation.setScriptExecutionDisabled` | stops the page from running scripts |
| `Emulation.setTimezoneOverride` | changes the time zone sites see |
| `Emulation.setSensorOverrideReadings` | changes sensor readings sites see |
| `Browser.setWindowBounds` | moves or resizes the browser window |
| `Browser.setContentsSize` | resizes the page contents |
| `Browser.addPrivacySandboxEnrollmentOverride` | changes Privacy Sandbox enrollment |
| `Browser.getBrowserCommandLine` | reads the browser launch switches |
| `Browser.executeBrowserCommand` | runs a browser command |
| `Target.setAutoAttach` | attaches to every new target (set it per target, not browser-wide) |
| `Target.disposeBrowserContext` | destroys a browser context and its tabs |
| `Tethering.bind` | opens or closes a port on the host that reaches the browser |
| `Tethering.unbind` | opens or closes a port on the host that reaches the browser |
| `FileSystem.getDirectory` | reads a directory handle of the file system |

Left free on purpose: device metrics, emulated media, CPU throttling and touch emulation (agents use them constantly), `Emulation.setLocaleOverride`, the Overlay domain, and `Browser.getHistograms` (large but harmless: `check cdp-domain` writes it to its own file). Writes to virtual hardware (`BluetoothEmulation`, `SmartCardEmulation`, every command), raw `Input` events, site storage reads and writes, credentials, request interception and body reads are in the table. Package code (plan `act` operations, recipes, flows that carry `acknowledge`) is not a raw call and is not gated here.

**Wrapped calls**: `Target.sendMessageToTarget` is free by itself but carries a raw method in its `message`. The gate parses the inner JSON and judges the inner method exactly as a direct call: its permission tier, its `url` scheme and its params. A `message` that is not a JSON object with a string `method` is refused.

**Deprecated twins**: a deprecated method inherits the policy of the method that replaced it. The live schema's `redirect` (followed through chains), or a same-named method in another domain, decides, so a twin Chrome adds later is covered (`enable` and `disable` never match by name: every domain has them); the rows above keep the known twins classified when no schema can be read.

**URL schemes**: a navigation to these schemes is refused, because they read local files or open browser internals (one table, `URL_SCHEME_POLICY` in `cdp-policy.mts`).

| Scheme | Why |
|---|---|
| `file:` | read files from the local disk |
| `chrome:` | open browser internals |
| `chrome-untrusted:` | open browser internals |
| `chrome-extension:` | open extension pages and files |
| `devtools:` | open browser internals |
| `view-source:` | read a page source through the browser |
| `javascript:` | run a script in the current page |
| `about:` | open browser internals; only `about:blank` is allowed |

The refusal covers plan `goto`, a `--new-tab` url, `open --url`, `group` urls and the `url` param of `Page.navigate`, `Target.createTarget` and `Network.loadNetworkResource`. Only a `cdp` call with `acknowledge: true` may use these; `goto`, `--new-tab` and `open --url` cannot carry the acknowledgement and stay refused. `data:` URLs stay allowed (same as writing inline HTML) and `about:blank` too. Runner scripts (`script`) are raw by design and are not filtered by scheme.

A plan is refused before any browser call when a literal method needs permission and has no `acknowledge`. Runner scripts (`script`) are raw by design: needsAck is not enforced in `cdp.send`. A script cannot ask the user, so this is the one place a hard stop remains: `cdp.send` refuses `Browser.close`, `Browser.crash`, `Browser.crashGpuProcess`, `Page.crash`, `Page.close`, `Security.setIgnoreCertificateErrors`, `Target.setRemoteLocations`, `Target.exposeDevToolsProtocol` and `Target.sendMessageToTarget` (a wrapped call cannot be acknowledged); send them with a `cdp` call and `acknowledge: true` after the user agrees.

## Dump or record a whole domain (`check cdp-domain`)

One call enables a domain and saves what it reports into files; stdout carries counts, notes and findings only, never results. Options (env-style names, as every recipe): `CDPDOM_DOMAIN` (required, e.g. `Performance`), `CDPDOM_MODE` (`dump`, `events` or `both`, default `both`), `CDPDOM_MS` (event window, at least 100, default 2000), `CDPDOM_METHODS` (JSON array of extra parameterless command names of the domain), `CDPDOM_URL` (loaded after the listeners attach and the domain is enabled, before the dump).

- The domain is checked against the installed Chrome: an unknown name prints `[FINDING] CDP_DOMAIN_UNKNOWN` with the nearest names and exits 1. The domain is enabled with its own `enable` (skipped with a note when it needs parameters; `Fetch` is never enabled automatically) before any load, because Network, Page, DOM and DOMStorage events are not replayed on a late enable.
- **Dump**: every command of the domain with no required parameter whose name starts with get, is, can, request, list or query, plus the parameterless commands named in `CDPDOM_METHODS`. Each call has its own deadline (a timeout is a row, not a failure), runs with scope `auto` (browser-level getters such as `SystemInfo.getInfo` work from a page), and is saved whole: `domain-<Domain>.json` maps method to result for results up to 64 KB, a larger result is its own file `domain-<Domain>/<method>.json`, and `domain-<Domain>-index.json` lists a row per method `{method, status: ok|error|timeout|skipped, reason?, bytes, file?, scope?}` (`bytes` is the size of the saved JSON). Methods the policy classifies as needing the user's permission and the hazards below are `skipped` with the reason; a Chrome error is a row and a `[FINDING] CDP_DOMAIN_METHOD_FAILED` with Chrome's text.
- **Events**: every event the schema lists for the domain is recorded for `CDPDOM_MS` into `domain-<Domain>.jsonl`, one `{ts, method, params}` per line with params whole (the file is mode 0600 and may contain tokens: treat it like HAR bodies). stdout lists `[EVENT] {event, count}` per event seen and one note naming the silent ones; Log, Audits, Media, Performance and Inspector may stay silent without a trigger.
- The last line is `[METRIC] CDP_DOMAIN domain=… methods=… ok=… skipped=… events=… failed=…`.

**Dump hazards** (one list, `CDP_DOMAIN_HAZARDS` in `src/engine/cdp-checks/cdp-domain-hazards.mts`; a test holds this table to it and to the live schema): the dump never sends these, even when `CDPDOM_METHODS` names them; each can crash, hang or strand the page, or change state.

| Method | Why |
|---|---|
| `Emulation.setDisplayFeaturesOverride` | crashes the browser |
| `Storage.deleteStorageBucket` | crashes the browser |
| `Input.synthesizePinchGesture` | hangs without answering |
| `Page.waitForDebugger` | hangs without answering |
| `Storage.runBounceTrackingMitigations` | hangs without answering |
| `Memory.getDOMCountersForLeakDetection` | leaves the page unresponsive |
| `Memory.prepareForLeakDetection` | leaves the page unresponsive |
| `Memory.forciblyPurgeJavaScriptMemory` | discards page memory |
| `Debugger.pause` | pauses page execution |
| `Debugger.resume` | changes page execution |
| `Debugger.stepInto` | changes page execution |
| `Debugger.stepOver` | changes page execution |
| `Debugger.stepOut` | changes page execution |
| `Runtime.terminateExecution` | stops running page scripts |
| `Runtime.runIfWaitingForDebugger` | releases a target waiting for its debugger |
| `Page.stopLoading` | cancels the page load |
| `Page.reload` | reloads the page |
| `Page.startScreencast` | starts a frame stream |
| `Page.startScreenRecording` | starts a recording |
| `Profiler.start` | starts a profile that stays on |
| `Profiler.startPreciseCoverage` | starts coverage that stays on |
| `HeapProfiler.startSampling` | starts a profile that stays on |
| `HeapProfiler.startTrackingHeapObjects` | starts tracking that stays on |
| `Tracing.start` | starts a trace that stays on |
| `Tracing.end` | ends a trace another client started |
| `Browser.resetPermissions` | resets every site permission |
| `Log.clear` | erases log entries |
| `Network.clearBrowserCache` | erases the browser cache |
| `ServiceWorker.stopAllWorkers` | stops every service worker |
| `Target.createBrowserContext` | creates a browser context |
| `Target.attachToBrowserTarget` | attaches a new session |

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
