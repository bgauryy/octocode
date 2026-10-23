# Launch and stealth

Load when launching with flags, a proxy, or tuning stealth. Why: Apply launch settings to the intended session. Launch flags only apply to a fresh browser process. If output says `"reused": true`, clean up or change port.

```bash
octocode-chrome-devtools /raw open --headless --port 9222 --url "<url>"          # isolated profile; native viewport defaults to 1280x720
octocode-chrome-devtools /raw open --port 9222 --url "<url>"                     # visible isolated profile, for user auth
octocode-chrome-devtools /raw open --profile Default --port 9222 --chromePath "<Chromium>"   # real profile: approval first, browser quit
octocode-chrome-devtools /raw open --headless --proxyServer "socks5://127.0.0.1:1080"
octocode-chrome-devtools /raw open --headless --port 9222 --enableFeatures WebMCP --url "<url>"   # Chrome 150+
```

- Proxy config example: `dist/engine/octocode-chrome-devtools.vpn.example.json`. Other flags: `--windowSize WxH`, `--userAgent`, `--chromePath` (only for non-standard installs), `--proxyBypassList`, `--proxyPacUrl`, `--config <proxy.json>`.
- Mobile: the window size sets only outer dimensions; also use CDP Emulation.
- Real profile (`--profile <dir>`): opens the data dir of the browser that `--chromePath` (or auto-detect) selects: Chrome, Canary, Chrome for Testing, Chromium or Brave. It fails, with no isolated fallback, when that browser is running, when the profile dir does not exist (it never creates one), with `--headless`, and on Google Chrome 136+, which [ignores remote debugging on its default data dir](https://developer.chrome.com/blog/remote-debugging-port). For Google Chrome, sign in inside an isolated session or move cookies with `cookies --from-port` / `--from-storage-state` (`intents.md#auth`).
- State lives under `<cwd>/.octocode/octocode-chrome-devtools/tmp/`: timestamped runs, `browser-state/` (profiles, session files), `session-meta/port-<N>/`.

## Stealth

Native browser settings are the default. `--stealth` opts into `undercover.mjs`: CDP overrides only, with no script injected into pages. It presents the host browser as itself without headless markers: the user agent and UA Client Hints lose `HeadlessChrome` (in navigator, workers and request headers), the reported screen grows to contain the window when headless reports a smaller one, and focus emulation keeps `document.hasFocus()` true. Platform, GPU, locale, timezone and viewport stay the host's, so every surface agrees. The overrides follow child targets (cross-origin iframes, dedicated and shared workers) and end with the run; a kept tab is native again on its next run without `--stealth`. A custom `--userAgent` is kept, and the UA Client Hints follow it: a string naming another OS than the host (Windows, macOS, Linux, Android, iOS, Chrome OS) gets that OS's `Sec-CH-UA-Platform`, `navigator.userAgentData.platform`, mobile flag, form factor, and OS-bound hints (platform version, architecture, bitness, device model); the host's own are never mixed in. A string naming no known OS drops the hints (`userAgentData` reports none) instead of describing the host. `navigator.platform` stays the host's (Chrome applies that override to pages but not workers, so changing it would split a worker from its page). Without `--stealth`, `--userAgent` only sets the string: the hints stay the host's.

Twelve self-checks then run in an isolated world: `webdriver`, headless markers, the UA, UA-CH and platform against the override, worker consistency, screen size, focus, plugins, languages and notification permission. A software WebGL renderer (SwiftShader, llvmpipe) is reported as `STEALTH_WARN`, not faked. The checks verify the emulation, not that a website accepts it.

| Case | Behavior |
|---|---|
| `--stealth --new-tab <url>` | Opens `about:blank`, applies, verifies, then navigates |
| `--stealth` on an attached tab | Reloads so the document loads under the overrides, then verifies; `--no-reload` keeps page state, verifies, and reports `STEALTH_RELOAD_SKIPPED` (the current document kept its native load headers) |
| `--no-stealth` | Disables opt-in emulation; combining it with `--stealth` is an error |
| Unsandboxed runner only | `CDP_STEALTH_ALLOW_FAIL=1` logs failures; `CDP_SKIP_STEALTH_VERIFY=1` skips verify |

Not covered: popups opened with `window.open` or `target=_blank` start with the native UA (attach to them with `--stealth`), service workers, and engine-level probes (CreepJS still scores headless Chrome partly headless). Smoke tests: `stealth-check` (bot.sannysoft.com) and `affiliates-stealth-check`. `octocode-scraping --provider cdp` applies the same overrides only with `--cdp-stealth`.

Next: launch problems → `recovery.md`; auth → `intents.md#auth`.
