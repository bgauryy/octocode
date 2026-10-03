# Recovery

Load when a run errors, returns nothing, or fails twice. Match the symptom. Common classes: consent wall, bot/CDN challenge, stale session, framework-controlled input, thin JS shell (run `actionability-diagnostics`, then scraping diagnostics).

## Launch and session

| Symptom | Fix |
|---|---|
| `Chrome not running on port` / `No page targets` | `open-browser.mjs --headless` first, or `--new-tab about:blank` |
| `Page.navigate` times out everywhere | Stale session: `--cleanup`, relaunch |
| Profile locked (cookie bridge) | Use `--from-port` / `--from-storage-state`, or quit Chrome |
| Need a cross-origin iframe (snapshot inlines only same-process frames) or worker | `--list-targets`, then `--target-url <pattern>` or `--target-type service_worker` |
| Long script killed | Sandbox `--script-timeout <ms>` (300000), runner `--timeout <ms>` (60000) |

## Runs

| Symptom | Fix |
|---|---|
| `STALE_SNAPSHOT_REF`; `DOM_BLOCKED not visible` after a hover/menu | Auto-recovered by role+name; else layout shifted: re-snapshot or use the `[NEW]` refs the last action printed |
| `ERR_ACCESS_DENIED` | Sandbox limits (`script-patterns.md`); `--verbose` lists allowed paths |
| `[CDP_RETRY_NEEDED]` (exit 2) / `CDP timeout for <method>` | Enable the domain or fix the method name; retry once |
| `VERIFY_MISMATCH` / framework ignores value | Try `DOM_ACTION=type` (keystrokes), then `DOM_INPUT=js` (native setter) |
| Bot/CDN challenge or CAPTCHA | Try a current desktop `--userAgent` once |
| Consent wall | Locate the control; after an authorized click, re-navigate |
| Headless text shows ligature gaps (`Sy tem One`) | Use `octocode-scraping` for clean text |

## Protocol

| Symptom | Fix |
|---|---|
| `Storage.enable` not found | Not needed; use `Network.getAllCookies`, `Runtime.evaluate`, `IndexedDB.*` |
| `IndexedDB.requestDatabaseNames` error | `IndexedDB.enable` + matching `securityOrigin`, or `indexedDB.databases()` |
| `getResponseBody` empty | Body evicted; read it on `responseReceived` |
| Zero DNS/TCP/TLS timings | Warm cache: `Network.clearBrowserCache` before navigating |
| FCP is null | Read paint entries after the final navigation settles; don't mix CDP lifecycle and `performance.now()` clocks |

Next: static fallback → `octocode-scraping`; otherwise apply the lobby stop rule.
