# Intents

Load when choosing what to capture or do. Why: Match the capture to the task. Use a ready check (`cdp-checks.md`) when it fits; combine checks or write a custom script for the evidence the task needs.

## Debug

- **page health**: measure trio + `measure-query` (`cdp-checks.md`).
- **network**: `network-measure-check`; custom scripts log URL, method, status, type, initiator, timing, never auth headers or cookies. Bodies: HAR section of `cdp-checks.md`.
- **console**: emit exceptions with source URL:line. **performance**: `MEASURE_URL=<url>`; clear cache for cold loads.
- **memory / coverage**: heap samples around one suspect action; coverage starts before navigation and stops at a stable state.

## Inspect

For many ambiguous saved records or images, load `clasify-screen`: exact fields and local text search first, then optional semantic scouting by reference. Use inline selection to find useful passages; use standalone typed predicates for positive-only resource decisions. Preserve unknown scope and verify deciding evidence.

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
- **WebMCP**: fresh Chrome 150+ with `--enableFeatures WebMCP`, then `WEBMCP_ACTION=list|invoke`. `WEBMCP_NO_TOOLS` is common; fall back to DOM. Use it when the page declares a suitable tool. Mutating tools need the same authorization as DOM actions.
- For an `octocode-scraping` corpus, CDP validates its graph actions and returns URLs/data to that corpus.

## Auth

- **login / user-auth**: visible Chrome (`open --url <url>`); the user signs in. Never automate passwords or MFA unless approved. Confirm success from a deterministic post-login URL or cookie name.
- **real profile** (`--profile`): warn that CDP can read cookies, tokens, and storage. Google Chrome 136+ refuses it (`launch-stealth.md`); Chromium and Chrome for Testing work.
- **cookie bridge**, preferred source order: same visible port (no transfer) → `--from-storage-state <jar>` → `--from-port <cdp>` → `--from-profile` (browser fully quit, approved; not Google Chrome 136+; `--chromePath` picks the browser). Values are never printed. The transfer keeps domain/host-only scope, path, expiry, flags and partition keys, reads the target back, and exits nonzero with `COOKIE_REJECTED` / `COOKIE_NOT_STORED` names when a cookie did not land. `--urls` keeps cookies that domain-match those hosts. Exports use the Playwright storage-state shape (mode 0600).

```bash
octocode-chrome-devtools /raw cookies --i-understand-secrets --from-port 9333 --to-port 9222 --urls "https://app.example.com"
octocode-chrome-devtools /raw cookies --i-understand-secrets --from-port 9333 --export-storage-state .octocode/octocode-chrome-devtools/tmp/auth.json --dry-run
```
Attachment requires a Chrome instance already exposing a remote-debugging endpoint. An ordinary Chrome session must be relaunched with remote debugging before attachment. After injecting, run the smallest check on `--to-port` with `--keep-tab`.

## Environment

- **emulate**: launch flags for window/proxy, then CDP Emulation (viewport, DPR, UA, locale, timezone, geolocation, network) before navigation. **inject**: `Page.addScriptToEvaluateOnNewDocument`, local code only. **monitor**: bounded duration, emit deltas.

Next: run the chosen check (`cdp-checks.md`) or write a script (`script-patterns.md`).
