# CDP Environment Intents

Load for emulation, injection, or monitoring. Why: environment changes must happen before navigation to be valid.

## emulate
Use launch flags for window/profile/proxy, then CDP Emulation for viewport, DPR, UA, locale, timezone, geolocation, touch, and network conditions. Apply before navigation.

## inject
Use `Page.addScriptToEvaluateOnNewDocument` for preload patches. Local scripts only; never fetch or import remote code. Feature-detect browser APIs before relying on them.

## monitor
Observe a page over time with a bounded duration. Emit deltas, errors, and metrics; always set an explicit timeout.

## Bot Walls
Apply stealth once for public sites likely to fingerprint headless Chrome, via `scripts/undercover.mjs`'s `applyStealthPatches(cdp)` — call it **before** `Page.navigate` (the injected script only applies to the next navigation). Self-test with `verifyStealth(cdp)`; re-verify against a real detection site before trusting it: `bot.sannysoft.com`, `bot.incolumitas.com`, `browserscan.net/bot-detection`, `deviceandbrowserinfo.com/are_you_a_bot`, `demo.fingerprint.com/web-scraping`. See `examples/stealth-check.mjs` for the full launch → patch → navigate → verify flow. If CAPTCHA or login persists, switch to visible user gate.

## Human-like Input
`scripts/human-input.mjs` builds Bezier-curve mouse movement, WPM-paced typing (with typo simulation), and wheel-scroll as CDP `Input.*` event sequences — trusted (`isTrusted:true`) input, unlike `dom-operations-check.mjs`'s JS-level `element.click()`/`.value=`. Use it when a target's behavioral anti-bot checks matter, not for routine form fills. Build a sequence (`buildHumanClickSequence`, `buildTypingEvents`, ...) and execute it with `runEventSequence(cdp, events)`.

Next: browser launch details in `references/chrome-flags.md`; recovery in `references/recovery.md`.
