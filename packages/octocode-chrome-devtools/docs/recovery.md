# Recovery

Load when a run errors or returns nothing. Why: Change the failing cause before retrying. Match the symptom. Common classes: consent wall, bot/CDN challenge, stale session, framework-controlled input, thin JS shell (run `actionability-diagnostics`, then scraping diagnostics).

## Bounded retry

The one retry rule for failed steps, timeouts and cancellations:

1. Inspect first: read the saved state and capture of the failed step. A mutation (click, type, submit, navigation with side effects) that failed, timed out or was cancelled mid-flight has an uncertain outcome until the state shows whether it applied.
2. Retry only after the cause changed (the fix from a table below, a refreshed ref, a longer deadline) and only a step the state shows did not apply. Never repeat an uncertain mutation automatically.
3. Stop after one changed retry of the same step. If it fails again, report the blocker as an SBAR (see Next, at the end).

## Launch and session

| Symptom | Fix |
|---|---|
| `Chrome not running on port` / `No page targets` | `open --headless` first, or `--new-tab about:blank` |
| `Page.navigate` times out everywhere | Inspect connection and target state first. For an owned isolated session, `cleanup`, then `open` and retry once; do not close an unrelated browser |
| `NO_TRACKED_SESSION` while a browser holds the port | `reason: NOT_PACKAGE_PROFILE` or `NO_SESSION_FILE`: cleanup didn't launch or record that browser, so it won't stop it; close it yourself or use another port. Cleanup from another cwd still finds isolated sessions |
| Profile locked (cookie bridge) | Use `--from-port` / `--from-storage-state`, or quit Chrome |
| Need a cross-origin iframe (snapshot inlines only same-process frames) or worker | `targets`, then `--target <id>`, `--target-url <pattern>` or `--target-type service_worker` |
| Long script killed | Sandbox `--script-timeout <ms>` (300000), runner `--timeout <ms>` (60000) |

## Runs

| Symptom | Fix |
|---|---|
| `STALE_SNAPSHOT_REF`; `DOM_BLOCKED not visible` after a hover/menu | Auto-recovered by role+name; else layout shifted: re-snapshot or use the `[NEW]` refs the last action printed |
| `ERR_ACCESS_DENIED` | Sandbox limits (`script-patterns.md`); `--verbose` lists allowed paths |
| `[CDP_RETRY_NEEDED]` (exit 2) / `CDP timeout for <method>` | Enable the required domain or fix the method/parameters before retrying |
| `VERIFY_MISMATCH` / framework ignores value | Try `DOM_ACTION=type` (keystrokes), then `DOM_INPUT=js` (native setter) |
| Bot/CDN challenge or CAPTCHA | Inspect the challenge; use visible `user-auth` if it persists |
| Consent wall | Locate the control; after an authorized click, re-navigate |
| Headless text shows ligature gaps (`Sy tem One`) | Use `octocode-scraping` for clean text |

## Protocol

| Symptom | Fix |
|---|---|
| `Storage.enable` not found | Not needed; use `Network.getAllCookies`, `Runtime.evaluate`, `IndexedDB.*` |
| `IndexedDB.requestDatabaseNames` error | `IndexedDB.enable` + matching `securityOrigin`, or `indexedDB.databases()` |
| `getResponseBody` empty | Body evicted; read it on `loadingFinished` after attaching before navigation |
| Zero DNS/TCP/TLS timings | Warm cache: `Network.clearBrowserCache` before navigating |
| FCP is null | Read paint entries after the final navigation settles; don't mix CDP lifecycle and `performance.now()` clocks |

Next: static fallback → `octocode-scraping`. If a blocker persists, report it as an SBAR: situation (target, symptom, failing step), background (attempts made and their capture paths), assessment (cause, or unresolved), recommendation (static fallback, or the user action needed).
