# Intents

Load when choosing what to capture or do. Pick one primary intent; a full audit is separate small scripts on one port, with findings merged in the answer. Prefer a ready check (`cdp-checks.md`) over custom code. Default when unsure: automate or debug/network.

## Debug

- **page health**: measure trio + `measure-query` (`cdp-checks.md`).
- **network**: `network-measure-check`; custom scripts log URL, method, status, type, initiator, timing, never auth headers or cookies. Bodies: HAR section of `cdp-checks.md`.
- **console**: emit exceptions with source URL:line. **performance**: `MEASURE_URL=<url>`; clear cache for cold loads.
- **memory / coverage**: heap samples around one suspect action; coverage starts before navigation and stops at a stable state.

## Inspect

- **security**: TLS, mixed content, CSP/security headers, cookie flags by name.
- **websocket**: URL, frame counts, redacted samples (not in HAR).
- **workers / service-worker**: `Target.getTargets` lists the whole browser; drop `chrome-extension://` and foreign origins.
- **intercept**: every paused request must continue, fail, or fulfill, or the page hangs.
- **screenshot**: `page-screenshot` (`cdp-checks.md`). **a11y / supply-chain**: report missing names and focus risks; list third-party origins, integrity, source maps.

## Storage and consent

`storage-measure-check` → `measure-query --view cookies|keys|findings`. Names, counts, and flags only unless values are approved. Consent: report banner selectors; click accept/reject only when asked.

## Automate

- `page-snapshot` → `DOM_REF=eN` on `dom-operations-check`; prefer refs over guessed CSS. Input is trusted by default; `fill` replaces text, `type` sends keystrokes (key handlers, autocomplete), `press DOM_KEY=Enter` submits. Read `[VERIFY]`: `MISMATCH` or `NO_VISIBLE_EFFECT` means the step did not land. After a hover/click that opens UI, act on the printed `[NEW] [eN]` refs without re-snapshotting. Icon-only or visual layouts: `SHOT_ANNOTATE=1` screenshot maps boxes to refs.
- A known multi-step sequence fits one `run(cdp)`.
- One meaningful mutation per step; confirm with a targeted check, not a new full snapshot. Listeners miss past events, so re-read current state.
- **WebMCP** (only when named): fresh Chrome 150+ with `--enableFeatures WebMCP`, then `WEBMCP_ACTION=list|invoke`. `WEBMCP_NO_TOOLS` is common; fall back to DOM. Mutating tools fall under the mutation gate.
- For an `octocode-scraping` corpus, CDP validates its graph actions and returns URLs/data to that corpus.

## Auth

- **login / user-auth**: visible Chrome (`open-browser.mjs --url <url>`); the user signs in. Never automate passwords or MFA unless approved. Confirm success from a deterministic post-login URL or cookie name.
- **real profile** (`--profile`): warn that CDP can read cookies, tokens, and storage.
- **cookie bridge**, preferred source order: same visible port (no transfer) → `--from-storage-state <jar>` → `--from-port <cdp>` → `--from-profile` (Chrome fully quit, approved). Values are never printed.

```bash
node $S/cookie-bridge.mjs --i-understand-secrets --from-port 9333 --to-port 9222 --urls "https://app.example.com"
node $S/cookie-bridge.mjs --i-understand-secrets --from-port 9333 --export-storage-state .octocode/tmp/auth.json --dry-run
```
This skill cannot attach to an ordinary open Chrome without relaunching it. After injecting, run the smallest check on `--to-port` with `--keep-tab`.

## Environment

- **emulate**: launch flags for window/proxy, then CDP Emulation (viewport, DPR, UA, locale, timezone, geolocation, network) before navigation. **inject**: `Page.addScriptToEvaluateOnNewDocument`, local code only. **monitor**: bounded duration, emit deltas.

Next: run the chosen check (`cdp-checks.md`) or write a script (`script-patterns.md`).
