# Chrome Flags Reference


## User-Agent override

`open-browser.mjs` does not override Chrome's user-agent by default. Add `--userAgent` only when the task needs a specific browser/device signature:

```
Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/<current-version>.0.0.0 Safari/537.36
```

```bash
node <skill-dir>/scripts/open-browser.mjs --headless \
  --userAgent "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/<current-version>.0.0.0 Safari/537.36"
```

Use a current version string that matches the installed browser when possible. A stale or cross-platform UA can be more fingerprintable than Chrome's default.


## Headless mode flags (inspection default)
Applied by `open-browser.mjs --headless`:

| Flag | Purpose |
|------|---------|
| `--remote-debugging-port=9222` | Enable CDP WebSocket on this port |
| `--headless=new` | Modern headless mode (Chrome 112+) — no visible window |
| `--user-data-dir=<tmpdir>/cdp-chrome-profile-9222` | Isolated temp profile — no user data read/written |
| `--disable-gpu` | Required for headless on many systems |
| `--disable-dev-shm-usage` | Prevents /dev/shm exhaustion in containers |
| `--no-sandbox` | Required for headless in some Linux/CI environments |
| `--no-first-run` | Skip Chrome's first-run setup dialog |
| `--no-default-browser-check` | Skip "make Chrome default" prompt |
| `--disable-background-mode` | Prevent Chrome from staying alive as a background app |
| `--user-agent=<ua>` | Optional UA override for specific compatibility or emulation tasks |

## Visible mode flags (user-requested only)
Applied when NOT headless (user says "show me", "open in my browser", "I want to see the page", "keep it open", "interactive"):

| Flag | Purpose |
|------|---------|
| `--remote-debugging-port=9222` | Enable CDP WebSocket |
| `--user-data-dir=<real-profile>` | Uses the user's real Chrome profile only when Chrome is not already running without CDP |
| `--profile-directory=Default` | Which profile to open when real-profile reuse is possible |
| `--restore-last-session` | Restore previous tabs when real-profile reuse is possible |

If Chrome is already running without CDP, `open-browser.mjs` launches an isolated visible CDP profile and returns `"isolated": true`. Existing cookies/extensions will not be available; complete auth in that visible CDP window, or close Chrome first if real-profile reuse is required.

## Mobile / viewport emulation

Mobile emulation works at **two levels** — always prefer the script level for full accuracy:

| Level | Flag / Method | Controls | Accuracy |
|---|---|---|---|
| **Launch-level** | `open-browser.mjs --windowSize 390x844` | Initial OS window size only — does **not** set `window.innerWidth` in `headless=new` (Chrome 112+) | Unreliable for viewport — no DPR, no touch |
| **Script-level** | `Emulation.setDeviceMetricsOverride` + `setTouchEmulationEnabled` + `setUserAgentOverride` | Viewport, DPR, mobile mode, touch events, UA + Sec-CH-UA hints | Full |

**Launch-level usage** (sets window before CDP attaches — optional, good for screenshots):
```bash
# Mobile window size
node <skill-dir>/scripts/open-browser.mjs --headless --windowSize 390x844

# Custom desktop size
node <skill-dir>/scripts/open-browser.mjs --headless --windowSize 1440x900

# Mobile window + mobile UA at launch
node <skill-dir>/scripts/open-browser.mjs --headless --windowSize 390x844 \
  --userAgent "Mozilla/5.0 (iPhone; CPU iPhone OS 17_4 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.4 Mobile/15E148 Safari/604.1"
```

**`--windowSize` does not set `window.innerWidth` in `headless=new`** (Chrome 112+). The OS window size and the page viewport are decoupled in modern headless mode. For any test that reads `window.innerWidth`, `window.innerHeight`, or uses CSS media queries, you must use the script-level approach.

For real mobile emulation (media queries, touch, DPR, Sec-CH-UA), always use the script-level CDP calls from `INTENTS_ENVIRONMENT.md` -> `## emulate`. Launch flags alone are **not enough**.


## Optional flags for special scenarios
Pass these via a custom launch call when needed:

| Flag | When to use |
|------|------------|
| `--disable-web-security` | Cross-origin testing (use only on isolated profiles) |
| `--allow-running-insecure-content` | Test mixed HTTP/HTTPS scenarios |
| `--ignore-certificate-errors` | Test self-signed cert environments |
| `--proxy-server=<host:port>` | Route traffic through a proxy |
| `--window-size=W,H` | Initial window dimensions (use via `--windowSize` in open-browser.mjs) |
| `--force-dark-mode` | Test dark-mode rendering |
| `--lang=<locale>` | Test localisation (e.g. `--lang=fr`) |
| `--disable-extensions` | Clean run without user extensions |
| `--incognito` | No persistent state — good for auth flow testing |

## Proxy Routing

Chrome can route traffic only through an already-configured proxy endpoint (HTTP/SOCKS/PAC), not directly into a VPN account. Use this only when the user provides or already runs a proxy.

```bash
node <skill-dir>/scripts/open-browser.mjs --headless \
  --proxyServer "socks5://127.0.0.1:1080" \
  --proxyBypassList "<-loopback>"

node <skill-dir>/scripts/open-browser.mjs --headless \
  --proxyPacUrl "http://127.0.0.1:3128/proxy.pac"
```

Proxy settings only apply when Chrome is launched fresh. If output has `"reused": true` and `"proxyRequested": true`, cleanup the tracked session first or use a different `--port`; Chrome cannot apply new proxy flags to an already-running CDP session.

Config shape for `<repo>/.octocode/chrome-devtools.json` or global `chromeDevtools.proxy`:

```json
{
  "proxy": {
    "enabled": true,
    "server": "socks5://127.0.0.1:1080",
    "bypassList": "<-loopback>"
  }
}
```

## Chrome binary paths per platform

`open-browser.mjs` auto-detects Chrome. If detection fails, pass `--chromePath`:

| Platform | Default paths checked |
|----------|-----------------------|
| macOS | `/Applications/Google Chrome.app/Contents/MacOS/Google Chrome`, `/Applications/Chromium.app/Contents/MacOS/Chromium` |
| Linux | `google-chrome`, `google-chrome-stable`, `chromium-browser`, `chromium` (searched in `$PATH`) |
| Windows | `C:\Program Files\Google\Chrome\Application\chrome.exe`, `C:\Program Files (x86)\Google\Chrome\Application\chrome.exe` |

```bash
# macOS/Linux — override path
node <skill-dir>/scripts/open-browser.mjs --headless --chromePath "/usr/bin/chromium-browser"

# Windows PowerShell — override path
node <skill-dir>/scripts/open-browser.mjs --headless --chromePath "C:\Program Files\Google\Chrome\Application\chrome.exe"
```

**Linux/CI notes:**
- `--no-sandbox` is required when running as root or inside Docker
- `--disable-dev-shm-usage` prevents crashes in low-memory containers
- For Chromium in CI: install with `apt-get install -y chromium-browser` (Ubuntu/Debian)

**Mode decision** → see `SKILL.md` → Open Browser.

---

## Shell Examples

Set these once in bash/zsh examples:

```bash
SKILL_DIR=<skill-dir>
TMPDIR=$(node -e "process.stdout.write(require('os').tmpdir())")
PORT=9222
```

Cleanup when a headless or isolated CDP session is no longer needed:

```bash
node "$SKILL_DIR/scripts/open-browser.mjs" --port "$PORT" --cleanup
```

Windows equivalents for the common command shape:

```powershell
# PowerShell
$SKILL_DIR = "<skill-dir>"
$TMPDIR    = node -e "process.stdout.write(require('os').tmpdir())"
node "$SKILL_DIR\scripts\open-browser.mjs" --headless --port 9222
node "$SKILL_DIR\scripts\cdp-sandbox.mjs" "$TMPDIR\cdp-task.mjs" --new-tab "about:blank" `
  > "$TMPDIR\cdp-output-task.txt" 2>&1
```

```cmd
SET SKILL_DIR=<skill-dir>
FOR /F "delims=" %%i IN ('node -e "process.stdout.write(require('os').tmpdir())"') DO SET TMPDIR=%%i
node "%SKILL_DIR%\scripts\open-browser.mjs" --headless --port 9222
node "%SKILL_DIR%\scripts\cdp-sandbox.mjs" "%TMPDIR%\cdp-task.mjs" --new-tab "about:blank" ^
  > "%TMPDIR%\cdp-output-task.txt" 2>&1
```

### One-pass inspection — headless

```bash
node "$SKILL_DIR/scripts/open-browser.mjs" --headless --port "$PORT"
# save the selected intent/pattern script to "$TMPDIR/cdp-task.mjs"
node "$SKILL_DIR/scripts/cdp-sandbox.mjs" "$TMPDIR/cdp-task.mjs" \
  --port "$PORT" --new-tab "about:blank" \
  > "$TMPDIR/cdp-output-task.txt" 2>&1
```

Use this for network, console, performance, memory, security, screenshot, and most audit scripts. Analyze the prefixes requested by that intent, for example `[NETWORK_ERROR]`, `[CONSOLE:ERROR]`, `[EXCEPTION]`, `[PERFORMANCE]`, `[FINDING]`.

### Visible browser / live page

```bash
node "$SKILL_DIR/scripts/open-browser.mjs" --url "https://site.com" --port "$PORT"
# after the user interacts, save an on-demand script to "$TMPDIR/cdp-live-check.mjs"
node "$SKILL_DIR/scripts/cdp-sandbox.mjs" "$TMPDIR/cdp-live-check.mjs" \
  --port "$PORT" --target-url "site.com" --keep-tab \
  > "$TMPDIR/cdp-output-live-check.txt" 2>&1
```

### User login -> authenticated follow-up

Browser must be visible. Ask before using the real profile.

```bash
node "$SKILL_DIR/scripts/open-browser.mjs" --profile Default --port "$PORT"
# save the user-auth script from INTENTS_AUTH.md to "$TMPDIR/cdp-user-auth.mjs"
node "$SKILL_DIR/scripts/cdp-sandbox.mjs" "$TMPDIR/cdp-user-auth.mjs" \
  --port "$PORT" --new-tab "about:blank" --keep-tab \
  > "$TMPDIR/cdp-auth-output.txt" 2>&1
# after [AUTH_COMPLETE], run debug/scrape/security scripts on the same port
```

### Inspect iframes and service workers

```bash
node "$SKILL_DIR/scripts/cdp-sandbox.mjs" --list-targets --port "$PORT"
node "$SKILL_DIR/scripts/cdp-sandbox.mjs" "$TMPDIR/cdp-task.mjs" \
  --port "$PORT" --target-url "iframe-url-pattern"
node "$SKILL_DIR/scripts/cdp-sandbox.mjs" "$TMPDIR/cdp-task.mjs" \
  --port "$PORT" --target-type service_worker
```

### Mobile / responsive layout

```bash
node "$SKILL_DIR/scripts/open-browser.mjs" --headless --port "$PORT" \
  --windowSize 390x844 \
  --userAgent "Mozilla/5.0 (iPhone; CPU iPhone OS 17_4 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.4 Mobile/15E148 Safari/604.1"
# still set script-level emulation in the CDP script for viewport/DPR/touch accuracy
node "$SKILL_DIR/scripts/cdp-sandbox.mjs" "$TMPDIR/cdp-emulate.mjs" \
  --port "$PORT" --new-tab "about:blank" \
  > "$TMPDIR/cdp-output-emulate.txt" 2>&1
```
