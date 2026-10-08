# Launch and stealth

Load when launching with flags, a proxy, or tuning stealth. Why: Apply launch settings to the intended session. Launch flags only apply to a fresh browser process. If output says `"reused": true`, clean up or change port.

```bash
node "$CDP" open --headless --port 9222 --url "<url>"          # isolated profile; native viewport defaults to 1280x720
node "$CDP" open --port 9222 --url "<url>"                     # visible isolated profile, for user auth
node "$CDP" open --profile Default --port 9222                 # real profile: approval first, Chrome quit
node "$CDP" open --headless --proxyServer "socks5://127.0.0.1:1080"
node "$CDP" open --headless --port 9222 --enableFeatures WebMCP --url "<url>"   # Chrome 150+
```

- Proxy config example: `dist/engine/octocode-chrome-devtools.vpn.example.json`. Other flags: `--windowSize WxH`, `--userAgent`, `--chromePath` (only for non-standard installs), `--proxyBypassList`, `--proxyPacUrl`, `--config <proxy.json>`.
- Mobile: the window size sets only outer dimensions; also use CDP Emulation.
- State lives under `<cwd>/.octocode/tmp/chrome-devtools/`: timestamped runs, `browser-state/` (profiles, session files), `session-meta/port-<N>/`.

## Stealth

Native browser settings are the default. `--stealth` opts into `undercover.mjs` and 15 patch self-checks. These checks verify emulation, not that a website accepts it. Emulation changes platform, viewport, locale, timezone and location; it can bias performance and locale tests. It never grants camera, microphone or notification permissions.

| Case | Behavior |
|---|---|
| `--stealth --new-tab <url>` | Opens `about:blank`, patches, verifies, then navigates |
| `--stealth` on an attached tab | Reloads after patching; `--no-reload` keeps page state and skips verify (patches from the previous run still hold) |
| `--no-stealth` | Disables opt-in emulation |
| Unsandboxed runner only | `CDP_STEALTH_ALLOW_FAIL=1` logs failures; `CDP_SKIP_STEALTH_VERIFY=1` skips verify |

Smoke tests: `stealth-check` (bot.sannysoft.com) and `affiliates-stealth-check`. `octocode-scraping --provider cdp` uses the same patches unless `--no-cdp-stealth`.

Next: launch problems → `recovery.md`; auth → `intents.md#auth`.
