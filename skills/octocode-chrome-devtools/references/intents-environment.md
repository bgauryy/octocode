# CDP environment intents

Load for emulation, injection, or monitoring. Why: environment changes must happen before navigation to be valid.

## emulate
Use launch flags for window/profile/proxy, then CDP Emulation for viewport, DPR, UA, locale, timezone, geolocation, touch, and network conditions. Apply before navigation.

## inject
Use `Page.addScriptToEvaluateOnNewDocument` for preload patches. Local scripts only; never fetch or import remote code. Feature-detect browser APIs before relying on them.

## monitor
Observe a page over time with a bounded duration. Emit deltas, errors, and metrics; always set an explicit timeout.

## Bot Walls
Opt-in only for **disabling**: stealth is **mandatory** on every CDP run through `cdp-runner.mjs` (`scripts/mandatory-stealth.mjs`). See `references/stealth-mandatory.md`. `open-browser.mjs` launches raw Chrome; patches apply when the runner attaches. Call `applyStealthPatches` in custom scripts only if you bypass the runner (not recommended). Self-test: `verifyStealth(cdp)`. Full flow: `scripts/cdp-checks/stealth-check.mjs`, `scripts/cdp-checks/affiliates-stealth-check.mjs`. If CAPTCHA or login persists after stealth, switch to visible user gate.

## Human-like Input
`scripts/human-input.mjs` builds Bezier-curve mouse movement, WPM-paced typing (with typo simulation), and wheel-scroll as CDP `Input.*` event sequences — trusted (`isTrusted:true`) input, unlike `dom-operations-check.mjs`'s JS-level `element.click()`/`.value=`. Use it when a target's behavioral anti-bot checks matter, not for routine form fills. Build a sequence (`buildHumanClickSequence`, `buildTypingEvents`, ...) and execute it with `runEventSequence(cdp, events)`.

Next: browser launch details in `references/chrome-flags.md`; recovery in `references/recovery.md`.
