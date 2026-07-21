# Chrome Flags Reference

Load when launching Chrome with special profile, proxy, binary, headless, or mobile needs. Why: launch flags only apply to a fresh browser process.

## Defaults
Headless inspection uses an isolated `.octocode/chrome-devtools/browser-state/` profile. Visible mode is for user-driven auth/live-page work. Real profile mode requires explicit user approval.

## Common Launches
```bash
node <skill-dir>/scripts/open-browser.mjs --headless --port 9222 --url "<url>"
node <skill-dir>/scripts/open-browser.mjs --url "<url>" --port 9222
node <skill-dir>/scripts/open-browser.mjs --profile Default --port 9222
node <skill-dir>/scripts/open-browser.mjs --headless --proxyServer "socks5://127.0.0.1:1080"
node <skill-dir>/scripts/open-browser.mjs --port 9222 --cleanup --dry-run
```

## Proxy
Proxy flags require a fresh launch. If output says `"reused": true` with `"proxyRequested": true`, cleanup or change port.

## Mobile
Use launch window size only for outer dimensions; still set CDP Emulation for viewport, DPR, touch, UA, locale, timezone, geolocation, and network.

## Binary Path
Prefer auto-detection. Override only when Chrome is nonstandard; quote paths in shell commands.

## Output
Generated scripts and logs go under `.octocode/tmp`; browser state and session metadata go under `.octocode/chrome-devtools` or global `~/.octocode/chrome-devtools` fallback.

Next: after launch, route by `references/INTENTS.md`.
