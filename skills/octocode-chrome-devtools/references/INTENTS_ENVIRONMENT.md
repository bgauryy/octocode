# CDP Environment Intents

Load for emulation, injection, or monitoring. Why: environment changes must happen before navigation to be valid.

## emulate
Use launch flags for window/profile/proxy, then CDP Emulation for viewport, DPR, UA, locale, timezone, geolocation, touch, and network conditions. Apply before navigation.

## inject
Use `Page.addScriptToEvaluateOnNewDocument` for preload patches. Local scripts only; never fetch or import remote code. Feature-detect browser APIs before relying on them.

## monitor
Observe a page over time with bounded duration. Emit deltas, errors, and metrics; do not leave long-running scripts without a timeout.

## Bot Walls
Apply stealth once for public sites likely to fingerprint headless Chrome. If CAPTCHA or login persists, switch to visible user gate.

Next: browser launch details in `references/CHROME_FLAGS.md`; recovery in `references/RECOVERY.md`.
