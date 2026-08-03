# CDP Environment Intents

Load for emulation, injection, or monitoring. Why: environment changes must happen before navigation to be valid.

## emulate
Use launch flags for window/profile/proxy, then CDP Emulation for viewport, DPR, UA, locale, timezone, geolocation, touch, and network conditions. Apply before navigation.

## inject
Use `Page.addScriptToEvaluateOnNewDocument` for preload patches. Local scripts only; never fetch or import remote code. Feature-detect browser APIs before relying on them.

## monitor
Observe a page over time with a bounded duration. Emit deltas, errors, and metrics; always set an explicit timeout.

## Bot Walls
Opt-in only — never apply by default. `open-browser.mjs` launches raw Chrome directly, so `navigator.webdriver` is already `false` without any patching. Apply stealth only when a specific public site fingerprints beyond that signal, via `scripts/undercover.mjs`'s `applyStealthPatches(cdp)`, called **before** `Page.navigate` (it only takes effect on the next navigation). Skip it otherwise: it patches ~15+ browser APIs, adds overhead, and an imperfect patch can read as more suspicious than none. Self-test with `verifyStealth(cdp)` against a real detection site (`bot.sannysoft.com`, or `bot.incolumitas.com` if that's down). Full flow: `examples/stealth-check.mjs`. If CAPTCHA or login persists, switch to visible user gate.

## Human-like Input
`scripts/human-input.mjs` builds Bezier-curve mouse movement, WPM-paced typing (with typo simulation), and wheel-scroll as CDP `Input.*` event sequences — trusted (`isTrusted:true`) input, unlike `dom-operations-check.mjs`'s JS-level `element.click()`/`.value=`. Use it when a target's behavioral anti-bot checks matter, not for routine form fills. Build a sequence (`buildHumanClickSequence`, `buildTypingEvents`, ...) and execute it with `runEventSequence(cdp, events)`.

Next: browser launch details in `references/chrome-flags.md`; recovery in `references/recovery.md`.
