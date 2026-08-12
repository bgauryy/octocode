# Recovery Reference

Hermetic evals do not cover every real bot-wall/CAPTCHA/region case — after **two** same-class failures, stop and summarize. Common live classes: consent wall, bot/CDN challenge, stale CDP session, fill without React setters, thin JS shell (hand off to scrape diagnostics).


| Situation | Fix |
|-----------|-----|
| Filled a React/Vue-controlled input, `input`/`change` events fired, but the site's own search/submit/filter acted as if the field were still empty | Prototype-setter fix in `references/script-patterns-async.md#waitForSelector` — `dom-operations-check.mjs`'s fill action already does this. |
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
| Bot-wall detected (bot-wall / CDN challenge instead of real page) | Pass `--userAgent` explicitly with a current Chrome desktop UA string only when needed. For sites that fingerprint JS (canvas, WebGL, timing): use the `user-auth` flow — visible browser, user solves the challenge in the CDP-controlled session. |
| `ERR_ACCESS_DENIED` in sandbox | Script tried to write outside `cdp.outputDir`, read a blocked path, or spawn a child process / Worker. Fix: all file writes via `join(cdp.outputDir, filename)`; all browser interaction via `cdp.send()`; no `child_process`, `net`, or `new Worker()`. Rerun with `cdp-sandbox.mjs --verbose` to see the exact allowed fs/net paths. |
| `[AUTH_TIMEOUT]` — user-auth script timed out | User did not authenticate within `TIMEOUT_MS`. Increase the timeout, verify `POST_AUTH_PATTERN` matches the actual post-login URL fragment, or set `AUTH_COOKIE_NAME` to the exact cookie the app sets on successful login. |
| Events not firing, or `--new-tab <url>` misses network/script events | Tab loaded before listeners attached — use `--new-tab about:blank`, attach listeners, then call `Page.navigate` inside `run()` |
| JavaScript dialog blocking all commands | Add dialog guard before navigate: `cdp.on('Page.javascriptDialogOpening', () => cdp.send('Page.handleJavaScriptDialog', { accept: true }))` — see Dialog guard in `cdp-agent.md` section 0 |
| URL with `?` or `&` fails in zsh | Always quote the URL: `--url "http://..."` |
| `Runtime.evaluate` hangs after `Debugger.enable` | Add `await cdp.send('Debugger.setSkipAllPauses', { skip: true })` immediately after `Debugger.enable` |
| `Page.navigate` times out on ALL URLs | Chrome session is stale — run `open-browser.mjs --cleanup` then relaunch with `--headless` |
| Unsure whether cleanup will kill the tracked browser | Run `open-browser.mjs --cleanup --dry-run`; it reports whether the tracked PID matches both the CDP port and .octocode profile without killing anything |
| `Security.getSecurityState` not found, or `Security.securityStateChanged` listener never fires (no error, just silent) | Both removed/deprecated — listen for `Security.visibleSecurityStateChanged` instead |
| `Storage.enable` not found (exit 2) | Not available in Chrome CDP (Chrome 120+). Remove the call — cookies, localStorage, sessionStorage, and IndexedDB are accessible without it via `Network.getAllCookies`, `Runtime.evaluate`, and `IndexedDB.*` domain calls |
| `IndexedDB.requestDatabaseNames` error | Call `IndexedDB.enable` first and pass `securityOrigin` matching the page's origin — omitting either causes the error. `Runtime.evaluate` with `indexedDB.databases()` is a simpler one-call alternative. |
| `Target.createBrowserContext` not allowed | Requires browser-level WebSocket — not available in tab-level CDP connection |
| Geolocation `getCurrentPosition` hangs | Add `Browser.grantPermissions({ permissions: ["geolocation"] })` before `Emulation.setGeolocationOverride` |
| `CSS.enable` throws "DOM agent needs to be enabled first" | Enable `DOM` before `CSS` — order matters |
| Coverage shows 0 functions/rules | Target page has no JS/CSS frameworks — test on a real app page, not static HTML |
| **Consent / GDPR wall — page redirects to privacy dialog before content** | Detect: title in foreign language, request count < 20, no API calls seen. Fix: `const btn = [...document.querySelectorAll('button,a')].find(b => /accept\|agree\|לקבל/i.test(b.innerText\|b.textContent\|'')); if (btn) btn.click();` → wait 1500ms → re-navigate to original URL. Add this check after first `Page.navigate` settles. |
| Performance metrics show DNS/TCP/TLS = 0ms and all resource durations = 0ms | You are measuring a warm/cached navigation. For cold-load metrics: call `await cdp.send('Network.clearBrowserCache', {})` and `await cdp.send('Network.clearBrowserCookies', {})` before `Page.navigate`, or use `--headless` with a fresh profile (default). |
| FCP / First Paint is `null` after navigation | Paint entries only exist for the navigated frame. If you navigated twice (e.g. after accepting a consent wall), call `performance.getEntriesByType('paint')` immediately after the *second* navigate settles, not after waiting. **Recommended:** read `performance.getEntriesByType('paint').find(e => e.name === 'first-contentful-paint')?.startTime` — already in ms from `navigationStart`, no reference-frame conversion needed. **Alternative (CDP lifecycle):** call `Page.setLifecycleEventsEnabled({ enabled: true })` after `Page.enable`; use `'commit'` for `navStartTs` and `'firstContentfulPaint'` for FCP delta — do NOT mix with `performance.now()` (different reference frame). |
| JS dead-code findings are all single-letter names (`c`, `i`, `Tt`, `Ut`) | Bundle is minified — function names are mangled. Filter out names with `name.length <= 2` before emitting `[FINDING] DEAD_CODE`. To get readable names you need source maps: serve the site with `//# sourceMappingURL=` intact and use `Debugger.getScriptSource` + source map parsing. |
| Fetch mocking not intercepting | Call `Fetch.enable` with `patterns` BEFORE navigation — it must be active before requests start |
| Screenshot is blank / all black | Page not fully loaded — add a `setTimeout` wait after navigate before calling `captureScreenshot` |
| Heap snapshot times out | Large page — increase `--timeout` to 120000+ ms |
| `Network.getResponseBody` returns nothing | Body was already evicted from cache — capture the `requestId` in `Network.responseReceived` immediately |
