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

## Auth Without Full Profile
Prefer cookie transfer into isolated headless over long-lived real-profile CDP:

```bash
node <skill-dir>/scripts/cookie-bridge.mjs --i-understand-secrets \
  --from-port 9333 --to-port 9222 --urls "https://app.example.com"
```

`--from-profile` needs Chrome fully quit. See `references/cookie-bridge.md`.

## Proxy
Proxy flags require a fresh launch. If output says `"reused": true` with `"proxyRequested": true`, cleanup or change port.

## Mobile
Use launch window size only for outer dimensions; still set CDP Emulation for viewport, DPR, touch, UA, locale, timezone, geolocation, and network.

## Binary Path
Prefer auto-detection. Override only when Chrome is nonstandard; quote paths in shell commands.

## Output
Generated scripts and logs go under `.octocode/tmp`; browser state and session metadata go under `.octocode/chrome-devtools` or global `~/.octocode/chrome-devtools` fallback.

Next: after launch, route by `references/intents.md`.
