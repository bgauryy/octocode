# Launch and stealth

Load when launching with flags, a proxy, or tuning stealth. Launch flags only apply to a fresh browser process. If output says `"reused": true`, clean up or change port.

```bash
node $S/open-browser.mjs --headless --port 9222 --url "<url>"          # isolated profile, 1280x720
node $S/open-browser.mjs --port 9222 --url "<url>"                     # visible isolated profile, for user auth
node $S/open-browser.mjs --profile Default --port 9222                 # real profile: approval first, Chrome quit
node $S/open-browser.mjs --headless --proxyServer "socks5://127.0.0.1:1080"
node $S/open-browser.mjs --headless --port 9222 --enableFeatures WebMCP --url "<url>"   # Chrome 150+
```

- Other flags: `--windowSize WxH`, `--userAgent`, `--chromePath` (only for non-standard installs), `--proxyBypassList`, `--proxyPacUrl`, `--config <proxy.json>`.
- Mobile: the window size sets only outer dimensions; also use CDP Emulation.
- State lives under `<cwd>/.octocode/tmp/chrome-devtools/`: timestamped runs, `browser-state/` (profiles, session files), `session-meta/port-<N>/`. Prune with `prune-artifacts.mjs`.

## Stealth

Every sandbox/runner run applies `undercover.mjs` and runs 15 self-checks before `run(cdp)`; a failed check stops the run.

| Case | Behavior |
|---|---|
| `--new-tab <url>` | Opens `about:blank`, patches, verifies, then navigates |
| Attached tab | Reloads after patching; `--no-reload` keeps page state and skips verify (patches from the previous run still hold) |
| `--no-stealth` | Disables stealth (debug only) |
| Unsandboxed runner only | `CDP_STEALTH_ALLOW_FAIL=1` logs failures; `CDP_SKIP_STEALTH_VERIFY=1` skips verify |

Smoke tests: `stealth-check` (bot.sannysoft.com) and `affiliates-stealth-check`. `octocode-scraping --provider cdp` uses the same patches unless `--no-cdp-stealth`. Stealth does not solve CAPTCHAs; switch to visible user-auth.

Next: launch problems → `recovery.md`; auth → `intents.md#auth`.
