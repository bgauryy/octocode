# Examples


## Shell syntax note — multiplatform

All examples below use **bash/zsh** syntax (macOS / Linux).

**Windows PowerShell equivalents:**
```powershell
# PowerShell
$SKILL_DIR = "<skill-dir>"
$TMPDIR    = node -e "process.stdout.write(require('os').tmpdir())"
node "$SKILL_DIR\scripts\open-browser.mjs" --headless --port 9222
node "$SKILL_DIR\scripts\cdp-sandbox.mjs" "$TMPDIR\cdp-task.mjs" --new-tab "about:blank" `
  > "$TMPDIR\cdp-output-task.txt" 2>&1
node "$SKILL_DIR\scripts\open-browser.mjs" --port 9222 --cleanup
```

**Windows CMD equivalents:**
```cmd
SET SKILL_DIR=<skill-dir>
FOR /F "delims=" %%i IN ('node -e "process.stdout.write(require('os').tmpdir())"') DO SET TMPDIR=%%i
node "%SKILL_DIR%\scripts\open-browser.mjs" --headless --port 9222
node "%SKILL_DIR%\scripts\cdp-sandbox.mjs" "%TMPDIR%\cdp-task.mjs" --new-tab "about:blank" ^
  > "%TMPDIR%\cdp-output-task.txt" 2>&1
node "%SKILL_DIR%\scripts\open-browser.mjs" --port 9222 --cleanup
```

Note: Chrome path on Windows is auto-detected by `open-browser.mjs`. If it fails, pass `--chromePath "C:\Program Files\Google\Chrome\Application\chrome.exe"` explicitly. See `CHROME_FLAGS.md` for details.

## "Check network and console errors" — headless (default)

```bash
SKILL_DIR=<skill-dir>
TMPDIR=$(node -e "process.stdout.write(require('os').tmpdir())")
node "$SKILL_DIR/scripts/open-browser.mjs" --headless --port 9222
# → generate network+console script using SCRIPT_PATTERNS.md, save to $TMPDIR/cdp-network.mjs
node "$SKILL_DIR/scripts/cdp-sandbox.mjs" "$TMPDIR/cdp-network.mjs" --new-tab "about:blank" \
  > "$TMPDIR/cdp-output-network.txt" 2>&1
node "$SKILL_DIR/scripts/open-browser.mjs" --port 9222 --cleanup
# → analyze cdp-output-network.txt for [NETWORK_ERROR] + [CONSOLE:ERROR] + [EXCEPTION]
# → screenshots/metadata in $TMPDIR/.octocode-chrome-devtools/<timestamp>/
```

## "Open browser and let me see the page / login" — visible (user asked)

```bash
SKILL_DIR=<skill-dir>
TMPDIR=$(node -e "process.stdout.write(require('os').tmpdir())")
node "$SKILL_DIR/scripts/open-browser.mjs" --profile Default --port 9222
# → generate monitor script, save to $TMPDIR/cdp-monitor.mjs
node "$SKILL_DIR/scripts/cdp-sandbox.mjs" "$TMPDIR/cdp-monitor.mjs" \
  --new-tab "https://site.com" --keep-tab \
  > "$TMPDIR/cdp-output-monitor.txt" 2>&1
# → tab stays open for user interaction; analyze output when user is done
```

## "Audit performance and memory" — headless (default)

```bash
SKILL_DIR=<skill-dir>
TMPDIR=$(node -e "process.stdout.write(require('os').tmpdir())")
node "$SKILL_DIR/scripts/open-browser.mjs" --headless --port 9222
node "$SKILL_DIR/scripts/cdp-sandbox.mjs" "$TMPDIR/cdp-audit.mjs" --new-tab "about:blank" \
  > "$TMPDIR/cdp-output-audit.txt" 2>&1
node "$SKILL_DIR/scripts/open-browser.mjs" --port 9222 --cleanup
# → analyze cdp-output-audit.txt for [PERFORMANCE] + [FINDING] + [METRIC]
```

## "Inspect iframes and service workers" — headless (default)

```bash
SKILL_DIR=<skill-dir>
TMPDIR=$(node -e "process.stdout.write(require('os').tmpdir())")
# 1. List all targets (uses cdp-sandbox.mjs — --list-targets exits before running any script)
node "$SKILL_DIR/scripts/cdp-sandbox.mjs" --list-targets --port 9222
# 2. Connect to a specific iframe
node "$SKILL_DIR/scripts/cdp-sandbox.mjs" "$TMPDIR/cdp-task.mjs" \
  --target-url "iframe-url-pattern" --port 9222
# 3. Connect to service worker
node "$SKILL_DIR/scripts/cdp-sandbox.mjs" "$TMPDIR/cdp-task.mjs" \
  --target-type service_worker --port 9222
```

## "Search for a string across all page resources" — headless (default)

```bash
SKILL_DIR=<skill-dir>
TMPDIR=$(node -e "process.stdout.write(require('os').tmpdir())")
node "$SKILL_DIR/scripts/open-browser.mjs" --headless --port 9222
# → generate search script from SCRIPT_PATTERNS.md "Search Text Across All Resources"
# → set SEARCH_TERM and TARGET_URL in the script
node "$SKILL_DIR/scripts/cdp-sandbox.mjs" "$TMPDIR/cdp-search.mjs" --new-tab "about:blank" \
  > "$TMPDIR/cdp-output-search.txt" 2>&1
node "$SKILL_DIR/scripts/open-browser.mjs" --port 9222 --cleanup
```

## "Test on mobile / check responsive layout" — headless with mobile emulation

```bash
SKILL_DIR=<skill-dir>
TMPDIR=$(node -e "process.stdout.write(require('os').tmpdir())")
# Open with mobile window size (optional — script-level override is more accurate)
node "$SKILL_DIR/scripts/open-browser.mjs" --headless --port 9222 \
  --windowSize 390x844 \
  --userAgent "Mozilla/5.0 (iPhone; CPU iPhone OS 17_4 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.4 Mobile/15E148 Safari/604.1"
# → generate emulate script from INTENTS.md ## emulate → Pre-built emulate script
# → set DEVICE preset to "iPhone 15 Pro" and TARGET_URL in the script
node "$SKILL_DIR/scripts/cdp-sandbox.mjs" "$TMPDIR/cdp-emulate.mjs" --new-tab "about:blank" \
  > "$TMPDIR/cdp-output-emulate.txt" 2>&1
node "$SKILL_DIR/scripts/open-browser.mjs" --port 9222 --cleanup
# → look for [EMULATE] viewport active, [FINDING] LAYOUT_BREAK, [METRIC] innerWidth
# → screenshot saved to $TMPDIR/.octocode-chrome-devtools/<timestamp>/
```

## "Full security audit" — headless (default)

```bash
SKILL_DIR=<skill-dir>
TMPDIR=$(node -e "process.stdout.write(require('os').tmpdir())")
node "$SKILL_DIR/scripts/open-browser.mjs" --headless --port 9222
# → generate security audit script from SCRIPT_PATTERNS.md "Security Audit"
node "$SKILL_DIR/scripts/cdp-sandbox.mjs" "$TMPDIR/cdp-security.mjs" --new-tab "about:blank" \
  > "$TMPDIR/cdp-output-security.txt" 2>&1
node "$SKILL_DIR/scripts/open-browser.mjs" --port 9222 --cleanup
# → look for [FINDING] MISSING_CSP, WEAK_CSP, COOKIE_NO_HTTPONLY, PROTOTYPE_POLLUTION
# → screenshots/metadata in $TMPDIR/.octocode-chrome-devtools/<timestamp>/
```

## "Let me log in first, then scrape authenticated data" — visible (user-auth flow)

**Browser is visible** — never `--headless`. User completes auth manually; agent waits.

```bash
SKILL_DIR=<skill-dir>
TMPDIR=$(node -e "process.stdout.write(require('os').tmpdir())")

# 1. Open Chrome VISIBLY.
#    If output contains "isolated": true, log in in that CDP window.
node "$SKILL_DIR/scripts/open-browser.mjs" --profile Default --port 9222

# 2. Run user-auth script — agent waits while user authenticates
#    Set LOGIN_URL and POST_AUTH_PATTERN inside the script before running
node "$SKILL_DIR/scripts/cdp-sandbox.mjs" "$TMPDIR/cdp-user-auth.mjs" \
  --new-tab "about:blank" --keep-tab \
  > "$TMPDIR/cdp-auth-output.txt" 2>&1
# → watch $TMPDIR/cdp-auth-output.txt for [AUTH_COMPLETE] or [AUTH_TIMEOUT]
# → auth-state.json saved to $TMPDIR/.octocode-chrome-devtools/<timestamp>/

# 3. After [AUTH_COMPLETE] — run subsequent script on the SAME authenticated session
#    (generate scrape script from INTENTS.md ## scrape → Pre-built scrape script)
node "$SKILL_DIR/scripts/cdp-sandbox.mjs" "$TMPDIR/cdp-scrape.mjs" \
  --new-tab "about:blank" \
  > "$TMPDIR/cdp-scrape-output.txt" 2>&1
# → scraped data in $TMPDIR/.octocode-chrome-devtools/<timestamp>/

# 4. Do NOT close Chrome — leave it running for further authenticated tasks
#    When fully done: node "$SKILL_DIR/scripts/open-browser.mjs" --port 9222 --cleanup
```

**Agent decisions:**
- If output contains `[AUTH_COMPLETE]` → proceed to step 3
- If output contains `[AUTH_TIMEOUT]` → increase `TIMEOUT_MS` or re-check `POST_AUTH_PATTERN`
- If already authenticated in the CDP-controlled session → `[AUTH_COMPLETE]` emits immediately

**Combine with:** `user-auth` + `debug`, `user-auth` + `security` — use the same port and Chrome stays open between scripts.
