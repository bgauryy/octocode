# Browser Agent — Automation

Anti-detection scripts and human-like input helpers for the browser-agent subagent.

## Files

| File | Purpose |
|---|---|
| `stealth-inject.mjs` | 17 JS evasions via `Page.addScriptToEvaluateOnNewDocument` |
| `human-input.mjs` | Human-like mouse (Bezier curves), keyboard, scroll via CDP Input |
| `detection-check.mjs` | Self-test: score all bot-detection signals in the current page |

---

## Stealth Injection

### What it patches (17 evasions)

| # | Signal | What we do |
|---|---|---|
| 1 | `navigator.webdriver` | Delete property → `undefined` |
| 2 | `window.chrome` | Add runtime, csi, loadTimes |
| 3 | `chrome.app` | Add realistic app object with InstallState/RunningState |
| 4 | `navigator.plugins` | Spoof 3 real plugins (PDF, NaCl) |
| 5 | `navigator.mimeTypes` | Match the spoofed plugins |
| 6 | `navigator.languages` | Set `['en-US', 'en']` |
| 7 | `navigator.vendor` | Set `'Google Inc.'` |
| 8 | `navigator.hardwareConcurrency` | Set 4 (if < 4) |
| 9 | `Permissions.query` | Fix notifications → `'default'` not `'denied'` |
| 10 | `Notification.permission` | Set `'default'` if currently `'denied'` |
| 11 | `console.debug` | Ensure distinct from `console.log` |
| 12 | User-Agent | Strip `HeadlessChrome` (for old headless mode) |
| 13 | WebGL vendor/renderer | Report Intel GPU instead of SwiftShader |
| 14 | Canvas fingerprint | Add imperceptible 1-bit noise to `toDataURL` |
| 15 | iframe `contentWindow` | Patch `navigator.webdriver` inside iframes too |
| 16 | `screen` dimensions | Report 1920×1080 instead of 0×0 |
| 17 | `outerWidth/outerHeight` | Ensure outer >= inner (real browser behaviour) |
| 18 | `media.canPlayType` | Return `'probably'`/`'maybe'` for H.264/AAC codecs |

### How to use with chromeDebug

**Option A — inject scheme (automatic for all navigations):**
```
chromeDebug scheme:"inject"
  url:"https://example.com"
  scriptSource:"<paste STEALTH_SCRIPT from stealth-inject.mjs>"
  port:9222
```

**Option B — raw CDP (manual):**
```
chromeDebug scheme:"raw"
  method:"Page.addScriptToEvaluateOnNewDocument"
  params:{"source": "<STEALTH_SCRIPT>"}
  port:9222
```

Then navigate normally — the script runs before any page JS on every navigation.

### Critical: `--enable-automation` flag

Our `launchChrome` does NOT add `--enable-automation`. This is intentional — it's the primary signal that sets `navigator.webdriver = true`. Never add it.

Also avoid `--enable-unsafe-swiftshader` which reveals software WebGL rendering.

---

## Human-like Input

### Mouse movement

Uses cubic Bezier curves with:
- Random control point offsets
- Ease-in-out acceleration
- Gaussian wobble along path
- Overshoot + correction (30% of moves)
- Burst pauses (simulates natural rhythm)

### Keyboard

- WPM-based delay per character (±variance)
- Random burst pauses between groups
- 2% typo + backspace correction rate
- Special keys (Enter, Tab, Backspace) via keyDown/keyUp

### Usage pattern for browser-agent subagent

```
1. Get element position:
   scheme:"raw" method:"Runtime.evaluate"
   params:{expression:"JSON.stringify(document.querySelector('button').getBoundingClientRect())",returnByValue:true}
   → {x, y, width, height}

2. Execute move+click events (each as separate raw calls):
   buildHumanClickSequence(startX, startY, x + w/2, y + h/2)
   → array of {method, params, delayMs}
   
   For each event:
   scheme:"raw" method:event.method params:event.params
   (wait event.delayMs ms before next)

3. Type into focused input:
   buildTypingEvents("search query")
   → array of {method:"Input.insertText", params:{text:ch}, delayMs}
```

---

## Detection Check

Self-test the current page's bot-detection posture:

```
chromeDebug scheme:"raw"
  method:"Runtime.evaluate"
  params:{
    "expression": "<DETECTION_CHECK_SCRIPT from detection-check.mjs>",
    "returnByValue": true,
    "awaitPromise": true
  }
```

Returns JSON with:
- `score` / `total` / `pass_rate` / `verdict` (CLEAN / MOSTLY_CLEAN / DETECTED)
- Per-signal boolean (`webdriver_ok`, `chrome_ok`, `plugins_ok`, ...)
- `verdict`: `CLEAN` = all 14 checks pass

Run before any scraping task to verify stealth posture.

---

## Detection sites (for manual verification)

| Site | What it checks |
|---|---|
| `bot.sannysoft.com` | Classic: webdriver, plugins, languages, UA, Chrome runtime |
| `bot.incolumitas.com` | Behavioural + fingerprint |
| `browserscan.net/bot-detection` | Normal/Abnormal signal report |
| `deviceandbrowserinfo.com/are_you_a_bot` | isBot, hasWebdriverTrue, isHeadlessChrome, isAutomatedWithCDP |
| `demo.fingerprint.com/web-scraping` | FingerprintJS Pro |
| `recaptcha-demo.appspot.com/recaptcha-v3-request-scores.php` | reCAPTCHA v3 score |
