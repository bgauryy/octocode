# Chrome Flags Reference

Load when launching Chrome with special profile, proxy, binary, headless, or mobile needs. Why: launch flags only apply to a fresh browser process.

## Defaults
Headless inspection uses an isolated `.octocode/tmp/chrome-devtools/browser-state/` profile and a `1280x720` viewport (`--windowSize` overrides it) — Chrome's own headless default is a narrow, non-standard size that collapses responsive sites into their mobile layout. Visible mode is for user-driven auth/live-page work and keeps the OS's normal window sizing. Real profile mode requires explicit user approval.

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

## WebMCP (experimental)
Requires Chrome 150+ and a fresh launch — reused sessions can't add flags:
```bash
node <skill-dir>/scripts/open-browser.mjs --headless --port 9222 --enableFeatures WebMCP --url "<url>"
```
Workflow and fallback: `references/intents-automation.md#webmcp`.

## Output
Everything lands under `<workspace>/.octocode/tmp/chrome-devtools/`: run artifacts by timestamp, `browser-state/` for profiles and session tracking, `session-meta/port-<N>/` for per-port history. An unwritable workspace is an error; artifacts never fall back to a user-level Octocode home. Run `scripts/prune-artifacts.mjs` periodically to reclaim old runs — see `SKILL.md`.

Next: after launch, route by `references/intents.md`.
