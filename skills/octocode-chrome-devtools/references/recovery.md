# Recovery

Load when a run errors, returns nothing, or fails twice. Match the symptom; after two same-class failures stop and summarize. Common classes: consent wall, bot/CDN challenge, stale session, framework-controlled input, thin JS shell (run `actionability-diagnostics`, then scraping diagnostics).

## Launch and session

| Symptom | Fix |
|---|---|
| `Chrome not running on port` / `No page targets` | `open-browser.mjs --headless` first, or `--new-tab about:blank` |
| `--cleanup` says `NO_TRACKED_SESSION` | Launched from another cwd; rerun cleanup there |
| `Page.navigate` times out everywhere | Stale session: `--cleanup`, relaunch |
| Flags/proxy ignored, `"reused": true` | Fresh launch needed: cleanup or new port |
| `WebSocket unavailable` / `bad option: --allow-net` | Node 24+ required; `--allow-net` applies only on 25+ |
| Profile locked (cookie bridge) | Use `--from-port` / `--from-storage-state`, or quit Chrome |
| Need a cross-origin iframe (snapshot inlines only same-process frames) or worker | `--list-targets`, then `--target-url <pattern>` or `--target-type service_worker` |
| Long script killed | Sandbox `--script-timeout <ms>` (300000), runner `--timeout <ms>` (60000) |

## Runs

| Symptom | Fix |
|---|---|
| `Another locale override is already in effect` | Parallel runs on one kept tab; run sequentially (separate `--new-tab` runs are fine) |
| Fill or page state gone on the next run | Each attach reloads; pass `--no-reload` on follow-up steps |
| `STALE_SNAPSHOT_REF`; `DOM_BLOCKED not visible` after a hover/menu | Auto-recovered by role+name; else layout shifted: re-snapshot or use the `[NEW]` refs the last action printed |
| Measure shows `example.test` or zero requests | No `MEASURE_URL` ran the fixture; `MEASURE_EXISTING=1` misses past requests. Use `MEASURE_URL=<url>` |
| `ERR_ACCESS_DENIED` | Write only under `cdp.outputDir`; no `child_process`, `net`, workers. `--verbose` lists allowed paths |
| Check can't import a helper under `cdp-runner` | Run checks through `cdp-sandbox.mjs`, which stages helpers |
| `[CDP_RETRY_NEEDED]` (exit 2) / `CDP timeout for <method>` | Enable the domain or fix the method name; retry once |
| Listeners miss load events | `--new-tab about:blank`, attach, then `Page.navigate` in `run` |
| Dialog blocks every command | Dialog guard (`cdp-protocol.md`) |
| `Runtime.evaluate` hangs after `Debugger.enable` | `Debugger.setSkipAllPauses({skip:true})` |
| `VERIFY_MISMATCH` / framework ignores value | Try `DOM_ACTION=type` (keystrokes), then `DOM_INPUT=js` (native setter) |
| Bot/CDN challenge or CAPTCHA | Try a current desktop `--userAgent`; else visible user-auth |
| Consent wall | Locate the control, act only when authorized, re-navigate |
| Headless text shows ligature gaps (`Sy tem One`) | Use `octocode-scraping` for clean text |

## Protocol

| Symptom | Fix |
|---|---|
| `Security.getSecurityState` missing / no events | Listen for `Security.visibleSecurityStateChanged` |
| `Storage.enable` not found | Not needed; use `Network.getAllCookies`, `Runtime.evaluate`, `IndexedDB.*` |
| `IndexedDB.requestDatabaseNames` error | `IndexedDB.enable` + matching `securityOrigin`, or `indexedDB.databases()` |
| `CSS.enable`: DOM agent needed | Enable DOM first |
| `Fetch` not intercepting | `Fetch.enable` with patterns before navigation |
| `getResponseBody` empty | Body evicted; read it on `responseReceived` |
| Zero DNS/TCP/TLS timings | Warm cache: `Network.clearBrowserCache` before navigating |
| FCP is null | Read paint entries after the final navigation settles; don't mix CDP lifecycle and `performance.now()` clocks |

Next: still blocked after two tries → stop and report; static fallback → `octocode-scraping`.
