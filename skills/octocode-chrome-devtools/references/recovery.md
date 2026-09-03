# Recovery Reference

Load when a CDP call errors, returns empty, or the same failure class hits twice. Why: match the symptom to a known fix instead of retrying blind.

Hermetic checks do not cover every bot-wall/CAPTCHA/region case. After two same-class failures, stop, and summarize. Common classes: consent wall, bot/CDN challenge, stale session, framework-controlled fill, and thin JS shell (hand off to scrape diagnostics).
| Situation | Fix |
|-----------|-----|
| Framework-controlled input looks filled but the app ignores it | Use the prototype-setter fix in `references/script-patterns-async.md#waitForSelector`; `dom-operations-check.mjs` already does. |
| `Chrome not found` | Install Chrome or check path in `open-browser.mjs` |
| `Chrome not running on port` | Run `open-browser.mjs --headless` first |
| Chrome already open, no CDP | Handled automatically — `open-browser.mjs` launches isolated CDP session |
| `WebSocket unavailable` | Upgrade to Node.js 22+ (native WebSocket required, no install needed) |
| `bad option: --allow-net` from sandbox | `--allow-net` is Node **25+** only. Sandbox gates on `process.versions.node` major ≥ 25 (skips on 22–24). Pull latest `cdp-sandbox.mjs` if you still see this. |
| Cookie bridge / profile lock | Chrome already open with Default profile → use `--from-port` or `--from-storage-state`, or quit Chrome before `--from-profile`. |
| `Script not found` | Use `.octocode/tmp/cdp-<task>.mjs`, never hardcode `/tmp/` |
| `CDP timeout for <method>` | Domain not enabled — add the required `enable` call before using it |
| `No page targets found` | Use `--new-tab about:blank` to open a fresh tab |
| Need to inspect an iframe or service worker | Use `--list-targets` to discover, then `--target-url <pattern>` or `--target-type service_worker` |
| `[CDP_RETRY_NEEDED]` in output (exit 2) | Read the `[CDP_RETRY_NEEDED]` lines — fix the domain enable or method name, retry once |
| Bot/CDN challenge replaces the page | If needed, pass a current desktop Chrome `--userAgent`; for JS fingerprinting, use visible `user-auth`, and let you solve it. |
| `ERR_ACCESS_DENIED` in sandbox | Write only through `join(cdp.outputDir, filename)` and interact through `cdp.send()`; no `child_process`, `net`, or `new Worker()`. Use `--verbose` for allowed paths. |
| `[AUTH_TIMEOUT]` — user-auth script timed out | User did not authenticate within `TIMEOUT_MS`. Increase the timeout, verify `POST_AUTH_PATTERN` matches the actual post-login URL fragment, or set `AUTH_COOKIE_NAME` to the exact cookie the app sets on successful login. |
| Events not firing, or `--new-tab <url>` misses network/script events | Tab loaded before listeners attached — use `--new-tab about:blank`, attach listeners, then call `Page.navigate` inside `run()` |
| JavaScript dialog blocking all commands | Add dialog guard before navigate: `cdp.on('Page.javascriptDialogOpening', () => cdp.send('Page.handleJavaScriptDialog', { accept: true }))` — see Dialog guard in `cdp-agent.md` section 0 |
| URL with `?` or `&` fails in zsh | Always quote the URL: `--url "http://..."` |
| `Runtime.evaluate` hangs after `Debugger.enable` | Add `await cdp.send('Debugger.setSkipAllPauses', { skip: true })` immediately after `Debugger.enable` |
| `Page.navigate` times out on ALL URLs | Chrome session is stale — run `open-browser.mjs --cleanup` then relaunch with `--headless` |
| Unsure whether cleanup kills the tracked browser | Run `open-browser.mjs --cleanup --dry-run`; it reports whether the tracked PID matches both the CDP port and .octocode profile without killing anything |
| `Security.getSecurityState` not found, or `Security.securityStateChanged` listener never fires (no error, silent) | Both removed/deprecated — listen for `Security.visibleSecurityStateChanged` instead |
| `Storage.enable` not found (exit 2) | Not available in Chrome CDP (Chrome 120+). Remove the call — cookies, localStorage, sessionStorage, and IndexedDB are accessible without it through `Network.getAllCookies`, `Runtime.evaluate`, and `IndexedDB.*` domain calls |
| `IndexedDB.requestDatabaseNames` error | Call `IndexedDB.enable` first and pass `securityOrigin` matching the page's origin — omitting either causes the error. `Runtime.evaluate` with `indexedDB.databases()` is a simpler one-call alternative. |
| `Target.createBrowserContext` not allowed | Requires browser-level WebSocket — not available in tab-level CDP connection |
| Geolocation `getCurrentPosition` hangs | Add `Browser.grantPermissions({ permissions: ["geolocation"] })` before `Emulation.setGeolocationOverride` |
| `CSS.enable` throws "DOM agent needs to be enabled first" | Enable `DOM` before `CSS` — order matters | <!-- style-lint: ignore-line passive-voice -->
| Coverage shows 0 functions/rules | Target page has no JS/CSS frameworks — test on a real app page, not static HTML |
| Consent/GDPR wall appears before content | Detect title/request/API clues; locate a visible consent control, act only when authorized, wait for settlement, then re-navigate to the original URL. |
| Performance metrics show DNS/TCP/TLS = 0ms and all resource durations = 0ms | You are measuring a warm/cached navigation. For cold-load metrics: call `await cdp.send('Network.clearBrowserCache', {})` and `await cdp.send('Network.clearBrowserCookies', {})` before `Page.navigate`, or use `--headless` with a fresh profile (default). |
| FCP/First Paint is `null` | Read paint entries after the final navigation settles: `performance.getEntriesByType('paint').find(e => e.name === 'first-contentful-paint')?.startTime`. For CDP lifecycle timing, enable lifecycle events, and subtract `commit` from `firstContentfulPaint`; never mix that clock with `performance.now()`. |
| JS dead-code findings are all single-letter names (`c`, `i`, `Tt`, `Ut`) | Bundle is minified — function names are mangled. Filter out names with `name.length <= 2` before emitting `[FINDING] DEAD_CODE`. To get readable names you need source maps: serve the site with `//# sourceMappingURL=` intact and use `Debugger.getScriptSource` + source map parsing. | <!-- style-lint: ignore-line passive-voice -->
| Fetch mocking not intercepting | Call `Fetch.enable` with `patterns` BEFORE navigation — it must be active before requests start |
| Screenshot is blank / all black | Page not fully loaded — add a `setTimeout` wait after navigate before calling `captureScreenshot` |
| Heap snapshot times out | Large page — increase `--timeout` to 120000+ ms |
| `Network.getResponseBody` returns nothing | Body was already evicted from cache — capture the `requestId` in `Network.responseReceived` immediately |
