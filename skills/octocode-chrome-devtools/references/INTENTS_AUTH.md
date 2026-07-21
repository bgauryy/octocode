# CDP Auth Intent Details

## login

**Trigger phrases:** "login", "sign in", "log me in", "authenticate", "enter credentials", "fill the login form", "auth flow"

**Purpose:** Automate an authentication flow — navigate to login page, fill credentials, submit, verify auth success, capture session state. Combine with `security` to audit what tokens/cookies are set post-login.

**Domains:** `Page.enable`, `Runtime.enable`, `Network.enable`

**Key steps:**
1. Navigate to login URL
2. Wait for username/password fields
3. Fill credentials via `Runtime.evaluate` with real `input` + `change` events
4. Click submit or press Enter
5. Wait for redirect / dashboard element
6. Verify logged-in state
7. Optionally inventory auth cookie names and storage-token presence (combine with `security`)

**Selector discovery — when selectors are unknown:**
```js
// Find likely username field
const { result } = await cdp.send('Runtime.evaluate', {
  expression: `document.querySelector('input[type="email"], input[type="text"], input[name*="user"], input[name*="email"], input[id*="user"], input[id*="email"]')?.name`,
  returnByValue: true,
});

// Find likely password field
// input[type="password"] — usually the most reliable password selector
```

**Credential injection pattern (triggers React/Vue/framework state):**
```js
async function fillField(cdp, selector, value) {
  await cdp.send('Runtime.evaluate', {
    expression: `
      const el = document.querySelector(${JSON.stringify(selector)});
      const nativeInputValueSetter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set;
      nativeInputValueSetter.call(el, ${JSON.stringify(value)});
      el.dispatchEvent(new Event('input', { bubbles: true }));
      el.dispatchEvent(new Event('change', { bubbles: true }));
    `
  });
  console.log(\`[AUTOMATE] filled ${selector}\`);
}
```

**Post-login verification:**
```js
// Check URL changed (redirect after login)
const { result: url } = await cdp.send('Runtime.evaluate', { expression: 'location.href', returnByValue: true });
console.log(`[AUTOMATE] post-login URL: ${url.value}`);

// Check for auth cookie
const { cookies } = await cdp.send('Network.getAllCookies', {});
const authCookie = cookies.find(c => /session|token|auth/i.test(c.name));
if (authCookie) console.log(`[AUTOMATE] auth cookie set: ${authCookie.name}`);
else console.log('[FINDING] NO_AUTH_COOKIE: no session/token cookie after login');

// Check localStorage for token
const { result: token } = await cdp.send('Runtime.evaluate', {
  expression: `!!(localStorage.getItem('token') || localStorage.getItem('authToken') || localStorage.getItem('jwt'))`,
  returnByValue: true,
});
if (token.value) console.log('[AUTOMATE] token present in localStorage (value redacted)');
```

**`[FINDING]` conditions:**
- Login form not found → `[FINDING] LOGIN_FORM_NOT_FOUND: no username/password fields on ${url}`
- No redirect after submit → `[FINDING] LOGIN_NO_REDIRECT: URL unchanged after submit`
- Error message visible → `[FINDING] LOGIN_FAILED: error element present — "${errorText}"`
- No auth cookie and no token in storage → `[FINDING] NO_SESSION: login appeared to succeed but no session established`
- Auth request returned non-200 → `[FINDING] AUTH_HTTP_ERROR: ${status} on ${url}`

**Combine with:** `security` (audit tokens post-login), `network` (capture auth API calls), `debug` (catch login errors).

## user-auth

**Trigger phrases:** "let me log in myself", "I'll authenticate", "manual login", "open browser so I can sign in", "auth flow", "open visible browser", "I need to authenticate first", "let me sign in then scrape", "open browser non-headless for auth"

**Purpose:** Opens Chrome in **visible** mode (never headless), navigates to the login URL, then polls passively for authentication completion while the user completes any auth flow (password, 2FA, SSO, CAPTCHA, OAuth). Once auth is detected the browser **stays open** so all subsequent CDP scripts reuse the authenticated session.

**Key distinction from `login`:**
- `login` = agent injects credentials programmatically via CDP
- `user-auth` = user completes auth manually; agent waits and detects completion

**Key requirement:** Browser must be visible for the user to interact — do not pass `--headless` to `open-browser.mjs` for this intent.

**Domains required:** `Network`, `Page`, `Runtime`

**Critical flags:**
- `open-browser.mjs` → omit `--headless` (default is already non-headless; just don't add it)
- `cdp-sandbox.mjs` → add `--keep-tab` so the tab and session stay open after the script exits
- Do NOT call `cleanup()` in `run()` — the browser must stay alive for subsequent tasks



**Adaptive user-auth recipe:**

Use visible Chrome and keep the tab alive. Poll configurable signals and report names/booleans only, never cookie or token values.

```js
export async function run(cdp) {
  const LOGIN_URL = 'https://example.com/login';
  const POST_AUTH_PATTERN = '/dashboard';
  const AUTH_COOKIE_NAME = ''; // optional exact cookie name
  const TIMEOUT_MS = 120_000;

  await cdp.send('Network.enable', {});
  await cdp.send('Runtime.enable', {});
  await cdp.send('Page.enable', {});
  await cdp.send('Page.navigate', { url: LOGIN_URL });

  async function state() {
    const { result } = await cdp.send('Runtime.evaluate', {
      expression: `JSON.stringify({
        url: location.href,
        storageKeys: [...Object.keys(localStorage), ...Object.keys(sessionStorage)]
          .filter(k => /token|auth|jwt|session/i.test(k))
      })`,
      returnByValue: true,
    });
    const page = JSON.parse(result.value || '{}');
    const { cookies } = await cdp.send('Network.getAllCookies', {});
    const authCookies = cookies.filter(c => AUTH_COOKIE_NAME ? c.name === AUTH_COOKIE_NAME : /session|token|auth|jwt|sid/i.test(c.name));
    return { page, cookieNames: authCookies.map(c => c.name) };
  }

  const deadline = Date.now() + TIMEOUT_MS;
  while (Date.now() < deadline) {
    const s = await state();
    if ((POST_AUTH_PATTERN && s.page.url?.includes(POST_AUTH_PATTERN)) || s.cookieNames.length || s.page.storageKeys?.length) {
      console.log(`[AUTH_COMPLETE] url=${s.page.url} cookies=${s.cookieNames.join(',') || 'none'} storageKeys=${s.page.storageKeys?.join(',') || 'none'}`);
      return;
    }
    console.log(`[AUTH] waiting url=${s.page.url}`);
    await new Promise(r => setTimeout(r, 2000));
  }
  console.log('[AUTH_TIMEOUT] authentication not detected; tune POST_AUTH_PATTERN or AUTH_COOKIE_NAME');
}
```

Tune detection to the app. Prefer a post-login URL or exact cookie name over broad token heuristics when false positives are possible.

**Agent loop contract:**

```
REASON   → need authenticated session before running scrape / debug / security tasks
OPEN     → open-browser.mjs --profile Default --port 9222   ← NO --headless
WAIT     → run user-auth script with --keep-tab
           watch stdout for [AUTH_COMPLETE] or [AUTH_TIMEOUT]
CONTINUE → once [AUTH_COMPLETE], run subsequent scripts on the same port/session
REUSE    → Chrome stays open; subsequent scripts connect without re-authenticating
STOP WHEN: [AUTH_COMPLETE] emitted (or [AUTH_TIMEOUT] — handle appropriately)
```

**Shell shape:**
```bash
node "$SKILL_DIR/scripts/open-browser.mjs" --profile Default --port 9222
node "$SKILL_DIR/scripts/cdp-sandbox.mjs" "$TMPDIR/cdp-user-auth.mjs" \
  --new-tab "about:blank" --keep-tab \
  > "$TMPDIR/cdp-auth-output.txt" 2>&1
# After [AUTH_COMPLETE], run follow-up scripts on the same port.
```

**Combine with:** `scrape` (extract authenticated data), `security` (audit session tokens after login), `debug` (investigate auth-gated behavior), `network` (capture post-auth API calls).

**Auth detection signals (in precedence order):**
1. URL changes to `POST_AUTH_PATTERN` (most reliable — redirect after login)
2. Auth cookie set (name matches `/session|token|auth|jwt|sid/i` or `AUTH_COOKIE_NAME`)
3. `localStorage` / `sessionStorage` token key present

**Tuning guide:**
| Situation | Fix |
|---|---|
| `[AUTH_TIMEOUT]` — login redirects to subdomain | change `POST_AUTH_PATTERN` to a path fragment, not full URL |
| `[AUTH_TIMEOUT]` — SSO/SAML takes longer than 2 min | increase `TIMEOUT_MS` to `300_000` |
| False positive detection | set `AUTH_COOKIE_NAME` to the exact cookie set on your app |
| Already logged in in the CDP-controlled session | script auto-detects and emits `[AUTH_COMPLETE]` immediately |
