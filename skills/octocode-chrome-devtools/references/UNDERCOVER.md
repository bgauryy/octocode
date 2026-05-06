# Undercover Reference — Stealth CDP Techniques

Apply these before navigating to any site that might block headless browsers. All techniques use pure CDP — no external packages. Call `applyStealthPatches(cdp)` at the top of `run()`, before `Page.navigate`.

> **Rule:** If a site returns a bot-wall, CAPTCHA, 403, or 429 — or if findings are suspiciously empty — always retry with stealth patches applied first.

---

## Drop-in Stealth Function

Copy this block into any script. Call it once, before `Page.navigate`.

```js
async function applyStealthPatches(cdp, opts = {}) {
  const ua = opts.userAgent ??
    'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36';

  // ── 1. User-Agent + Client Hints ────────────────────────────────────────────
  await cdp.send('Network.setUserAgentOverride', {
    userAgent: ua,
    platform: 'Win32',
    userAgentMetadata: {
      brands: [
        { brand: 'Chromium',      version: '124' },
        { brand: 'Google Chrome', version: '124' },
        { brand: 'Not-A.Brand',   version: '99'  },
      ],
      fullVersion: '124.0.0.0',
      platform: 'Windows',
      platformVersion: '10.0.0',
      architecture: 'x86',
      model: '',
      mobile: false,
    },
  });

  // ── 2. Viewport — headless default is 0×0 outer, real Chrome is not ─────────
  await cdp.send('Emulation.setDeviceMetricsOverride', {
    width: 1920, height: 1080,
    deviceScaleFactor: 1,
    mobile: false,
    screenWidth: 1920, screenHeight: 1080,
    positionX: 0, positionY: 0,
  });

  // ── 3. Timezone + Locale ────────────────────────────────────────────────────
  await cdp.send('Emulation.setTimezoneOverride',  { timezoneId: opts.timezone ?? 'America/New_York' });
  await cdp.send('Emulation.setLocaleOverride',    { locale:     opts.locale   ?? 'en-US' });

  // ── 4. Geolocation ──────────────────────────────────────────────────────────
  await cdp.send('Emulation.setGeolocationOverride', {
    latitude:  opts.lat ?? 40.7128,
    longitude: opts.lon ?? -74.0060,
    accuracy: 100,
  });

  // ── 5. Grant permissions — avoids detection via blocked-permission signals ──
  await cdp.send('Browser.grantPermissions', {
    permissions: ['geolocation', 'notifications', 'camera', 'microphone'],
    origin: opts.origin ?? undefined,
  });

  // ── 6. Extra HTTP headers — look like a real browser ────────────────────────
  await cdp.send('Network.setExtraHTTPHeaders', {
    headers: {
      'Accept-Language':    'en-US,en;q=0.9',
      'Accept-Encoding':    'gzip, deflate, br',
      'sec-ch-ua':          '"Chromium";v="124", "Google Chrome";v="124", "Not-A.Brand";v="99"',
      'sec-ch-ua-mobile':   '?0',
      'sec-ch-ua-platform': '"Windows"',
    },
  });

  // ── 7. JS patches injected before any page script runs ──────────────────────
  await cdp.send('Page.addScriptToEvaluateOnNewDocument', {
    source: `
    (function () {

      // ── toString() native bypass ─────────────────────────────────────────────
      // MUST be first. Sophisticated detectors call Function.prototype.toString()
      // on patched getters to check if they're native. This makes all our patched
      // functions appear as native code when inspected.
      const _nativeToString = Function.prototype.toString;
      const _toStringProxy = new Proxy(_nativeToString, {
        apply(target, thisArg, args) {
          if (_patchedFns.has(thisArg)) return 'function () { [native code] }';
          return Reflect.apply(target, thisArg, args);
        },
      });
      Function.prototype.toString = _toStringProxy;
      const _patchedFns = new WeakSet();

      // Helper: define a property with a getter that appears native
      function defProp(obj, prop, getter) {
        _patchedFns.add(getter);
        try {
          Object.defineProperty(obj, prop, {
            get: getter,
            configurable: true,
            enumerable: true,
          });
        } catch (_) {}
      }

      // ── navigator.webdriver — the #1 automation tell ──────────────────────────
      defProp(navigator, 'webdriver', () => undefined);

      // ── navigator.vendor + platform ──────────────────────────────────────────
      // MISSING in many stealth setups. Detectors check both.
      defProp(navigator, 'vendor',   () => 'Google Inc.');
      defProp(navigator, 'platform', () => 'Win32');

      // ── navigator.maxTouchPoints — 0 on desktop, >0 on mobile ───────────────
      defProp(navigator, 'maxTouchPoints', () => 0);

      // ── window.chrome — stripped in headless; detectors check for it ─────────
      if (!window.chrome) {
        window.chrome = {
          runtime: {
            id: undefined,
            connect: () => {},
            sendMessage: () => {},
            onMessage: { addListener: () => {}, removeListener: () => {} },
          },
          app: { isInstalled: false },
          csi:        () => {},
          loadTimes: () => ({}),
        };
      }

      // ── navigator.plugins — empty in headless ────────────────────────────────
      defProp(navigator, 'plugins', () => Object.assign(
        [
          { name: 'Chrome PDF Plugin',   filename: 'internal-pdf-viewer',   description: 'Portable Document Format', length: 1 },
          { name: 'Chrome PDF Viewer',   filename: 'mhjfbmdgcfjbbpaeojofohoefgiehjai', description: '',              length: 1 },
          { name: 'Native Client',       filename: 'internal-nacl-plugin',  description: '',                         length: 2 },
        ],
        { namedItem: (n) => null, refresh: () => {}, item: (i) => null }
      ));

      // ── navigator.mimeTypes — empty in headless ──────────────────────────────
      defProp(navigator, 'mimeTypes', () => Object.assign(
        [{ type: 'application/pdf', suffixes: 'pdf', description: '', enabledPlugin: null }],
        { namedItem: () => null, item: () => null }
      ));

      // ── navigator.languages ──────────────────────────────────────────────────
      defProp(navigator, 'languages', () => ['en-US', 'en']);

      // ── navigator.hardwareConcurrency + deviceMemory ─────────────────────────
      defProp(navigator, 'hardwareConcurrency', () => 8);
      defProp(navigator, 'deviceMemory',        () => 8);

      // ── navigator.cookieEnabled ──────────────────────────────────────────────
      defProp(navigator, 'cookieEnabled', () => true);

      // ── navigator.permissions — headless returns 'denied' for notifications ──
      const _origQuery = navigator.permissions.query.bind(navigator.permissions);
      _patchedFns.add(_origQuery);
      navigator.permissions.query = function (p) {
        if (p.name === 'notifications') return Promise.resolve({ state: 'prompt', onchange: null });
        if (p.name === 'camera')        return Promise.resolve({ state: 'prompt', onchange: null });
        if (p.name === 'microphone')    return Promise.resolve({ state: 'prompt', onchange: null });
        return _origQuery(p);
      };
      _patchedFns.add(navigator.permissions.query);

      // ── navigator.connection — Network Information API ───────────────────────
      // Headless often exposes wrong or undefined values; real users have specific values.
      if (navigator.connection) {
        defProp(navigator.connection, 'rtt',            () => 50);
        defProp(navigator.connection, 'downlink',       () => 10);
        defProp(navigator.connection, 'effectiveType',  () => '4g');
        defProp(navigator.connection, 'saveData',       () => false);
      }

      // ── document.hasFocus() — always false in headless ──────────────────────
      // MISSING in most stealth setups. Detectors call this directly.
      const _origHasFocus = document.hasFocus.bind(document);
      document.hasFocus = function () { return true; };
      _patchedFns.add(document.hasFocus);

      // ── screen dimensions — 0 in headless ────────────────────────────────────
      defProp(window, 'outerWidth',  () => 1920);
      defProp(window, 'outerHeight', () => 1080);
      defProp(window, 'screenX',     () => 20);   // non-zero — window not at edge
      defProp(window, 'screenY',     () => 40);   // non-zero — window has OS chrome
      defProp(screen, 'width',       () => 1920);
      defProp(screen, 'height',      () => 1080);
      defProp(screen, 'availWidth',  () => 1920);
      defProp(screen, 'availHeight', () => 1040);
      defProp(screen, 'colorDepth',  () => 24);
      defProp(screen, 'pixelDepth',  () => 24);

      // ── WebGL vendor / renderer — SwiftShader is blacklisted ─────────────────
      const _patchWebGL = (ctx) => {
        const _orig = ctx.prototype.getParameter;
        _patchedFns.add(_orig);
        ctx.prototype.getParameter = function (p) {
          if (p === 37445) return 'Intel Inc.';               // UNMASKED_VENDOR_WEBGL
          if (p === 37446) return 'Intel Iris OpenGL Engine'; // UNMASKED_RENDERER_WEBGL
          return _orig.call(this, p);
        };
        _patchedFns.add(ctx.prototype.getParameter);
      };
      _patchWebGL(WebGLRenderingContext);
      if (typeof WebGL2RenderingContext !== 'undefined') _patchWebGL(WebGL2RenderingContext);

      // ── Canvas toDataURL — pixel noise breaks canvas hash fingerprint ─────────
      const _origToDataURL = HTMLCanvasElement.prototype.toDataURL;
      _patchedFns.add(_origToDataURL);
      HTMLCanvasElement.prototype.toDataURL = function (...args) {
        const ctx = this.getContext('2d');
        if (ctx && this.width > 0 && this.height > 0) {
          const img = ctx.getImageData(0, 0, 1, 1);
          img.data[0] ^= 1; // flip one bit — imperceptible, breaks hash
          ctx.putImageData(img, 0, 0);
        }
        return _origToDataURL.apply(this, args);
      };
      _patchedFns.add(HTMLCanvasElement.prototype.toDataURL);

      // ── Audio context fingerprint noise ───────────────────────────────────────
      // Detectors create an OfflineAudioContext, run oscillator, measure the buffer
      // hash. Patch AudioBuffer.getChannelData to add subtle noise.
      const _AudioBuffer = window.AudioBuffer;
      if (_AudioBuffer) {
        const _origGetChannelData = AudioBuffer.prototype.getChannelData;
        _patchedFns.add(_origGetChannelData);
        AudioBuffer.prototype.getChannelData = function (channel) {
          const data = _origGetChannelData.call(this, channel);
          // Modify only first sample — inaudible, defeats hash
          if (data.length > 0) data[0] += 1e-7 * Math.random();
          return data;
        };
        _patchedFns.add(AudioBuffer.prototype.getChannelData);
      }

      // ── MediaDevices.enumerateDevices() — returns [] in headless ─────────────
      // Real browsers return actual audio/video input device entries.
      if (navigator.mediaDevices && navigator.mediaDevices.enumerateDevices) {
        const _origEnum = navigator.mediaDevices.enumerateDevices.bind(navigator.mediaDevices);
        navigator.mediaDevices.enumerateDevices = function () {
          return _origEnum().then(real => real.length > 0 ? real : [
            { deviceId: 'default', kind: 'audioinput',  label: '',  groupId: 'default' },
            { deviceId: 'default', kind: 'audiooutput', label: '',  groupId: 'default' },
            { deviceId: 'default', kind: 'videoinput',  label: '',  groupId: 'default' },
          ]);
        };
        _patchedFns.add(navigator.mediaDevices.enumerateDevices);
      }

      // ── Iframe contentWindow.webdriver — detectors probe iframes ─────────────
      // BUG FIX: save original descriptor BEFORE redefining to avoid recursion.
      const _iframeDesc = Object.getOwnPropertyDescriptor(HTMLIFrameElement.prototype, 'contentWindow');
      if (_iframeDesc && _iframeDesc.get) {
        const _origContentWindowGetter = _iframeDesc.get;
        _patchedFns.add(_origContentWindowGetter);
        Object.defineProperty(HTMLIFrameElement.prototype, 'contentWindow', {
          get: function () {
            const win = _origContentWindowGetter.call(this);
            if (win) {
              try { Object.defineProperty(win.navigator, 'webdriver', { get: () => undefined }); } catch (_) {}
            }
            return win;
          },
          configurable: true,
        });
      }

    })();
    `,
  });

  console.log('[INJECT] Stealth patches applied (25 techniques)');
}
```

---

## Usage

```js
export async function run(cdp) {
  // Apply stealth BEFORE enabling domains or navigating
  await applyStealthPatches(cdp, {
    // optional overrides:
    // userAgent: '...',
    // timezone:  'Europe/London',
    // locale:    'en-GB',
    // lat: 51.5074, lon: -0.1278,
    // origin:    'https://example.com',
  });

  await cdp.send('Network.enable', {});
  await cdp.send('Runtime.enable', {});
  await cdp.send('Page.enable', {});

  await cdp.send('Page.navigate', { url: 'https://example.com' });
  await new Promise(r => setTimeout(r, 5000));
}
```

---

## Technique Reference

| # | Technique | How applied | What it fixes |
|---|-----------|-------------|---------------|
| 1 | User-Agent + Client Hints | `Network.setUserAgentOverride` + `userAgentMetadata` | UA mismatch, `sec-ch-ua` inconsistency |
| 2 | Real viewport dimensions | `Emulation.setDeviceMetricsOverride` | `outerWidth=0` headless tell |
| 3 | Timezone + Locale | `Emulation.setTimezoneOverride` + `Emulation.setLocaleOverride` | Locale/timezone mismatch |
| 4 | Geolocation | `Emulation.setGeolocationOverride` | Missing/null geolocation |
| 5 | Grant permissions | `Browser.grantPermissions` | `permissions.query` returning 'denied' |
| 6 | HTTP headers | `Network.setExtraHTTPHeaders` | Missing `sec-ch-ua*`, `Accept-Language` |
| 7 | **`toString()` native bypass** | `Page.addScriptToEvaluateOnNewDocument` | Detectors calling `.toString()` on patched functions to detect patching — must be first |
| 8 | Remove `navigator.webdriver` | JS patch | The #1 automation tell |
| 9 | `navigator.vendor` + `platform` | JS patch | `'Google Inc.'` / `'Win32'` absent in headless |
| 10 | `navigator.maxTouchPoints` | JS patch | Touch capability mismatch (0 = desktop) |
| 11 | Restore `window.chrome` | JS patch | Full `chrome.runtime` object absent in headless |
| 12 | Fix `navigator.plugins` | JS patch | Empty plugin list (3 fake entries) |
| 13 | Fix `navigator.mimeTypes` | JS patch | Empty mimeTypes list |
| 14 | Fix `navigator.languages` | JS patch | Language list mismatch |
| 15 | `hardwareConcurrency` + `deviceMemory` | JS patch | Hardware profile inconsistency |
| 16 | `navigator.cookieEnabled` | JS patch | May be false in isolated headless profiles |
| 17 | Fix `navigator.permissions` | JS patch | Notifications/camera/mic always 'denied' |
| 18 | `navigator.connection` | JS patch | Network Info API wrong values |
| 19 | **`document.hasFocus()` → true** | JS patch | Always `false` in headless — checked directly |
| 20 | Screen dimensions + `screenX/Y` | JS patch | `outerWidth/Height=0`, `screenX/Y=0` headless tells |
| 21 | WebGL vendor/renderer | JS patch | SwiftShader renderer is blacklisted |
| 22 | Canvas pixel noise | JS patch | Canvas hash fingerprint |
| 23 | **Audio context noise** | JS patch | `AudioBuffer` hash fingerprint |
| 24 | **`MediaDevices.enumerateDevices()`** | JS patch | Returns `[]` in headless |
| 25 | Iframe `contentWindow.webdriver` | JS patch (bug-fixed) | Webdriver leak through iframes |

---

## What stealth does NOT fix

These signals require infrastructure-level changes outside CDP:

| Signal | Why CDP cannot fix it | Workaround |
|--------|----------------------|------------|
| **TLS / JA3 / JA4** | Captured at the TCP/TLS handshake before any JS or CDP command runs | Route through a real-Chrome TLS stack or managed browser infra |
| **HTTP/2 SETTINGS frame** | Set by the browser network stack, not accessible via CDP | Same as above |
| **ASN / datacenter IP** | IP reputation checked server-side | Residential proxy |
| **CDP protocol artifacts** | Some detectors observe `Runtime.enable` serialization timing | CDP-free automation (nodriver) |
| **Site-specific behavioral ML** | Per-site model of what human navigation looks like | Human-like click/scroll + session warming |

---

## Recovery: Site Still Blocked After Stealth

| Signal | Next move |
|--------|-----------|
| 403 / CAPTCHA persists | TLS/JA3 is likely the blocker — stealth cannot fix this via CDP alone |
| Bot wall with keyboard/clipboard listeners | PerimeterX or Cloudflare — use visible mode with `--profile Default` |
| Page loads but content is empty / wrong | Login wall — switch to `user-auth` intent |
| Cloudflare Turnstile | Visible mode + real profile is the only CDP-only option |
