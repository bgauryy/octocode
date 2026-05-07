# CDP Storage And Consent Intent Details

## storage

**Trigger phrases:** "storage", "localStorage", "sessionStorage", "IndexedDB", "cookies", "cache storage", "service worker cache", "check what's stored", "what data does this site store", "storage quota", "offline data", "browser storage"

**Purpose:** Full metadata inventory of client-side storage — cookie names/flags, localStorage and sessionStorage keys/sizes, IndexedDB database names, Cache Storage caches, Service Worker registrations, and storage quota. Use for privacy audits, debugging persistence bugs, and session forensics without emitting stored secret values.

**Domains:** `Network.enable`, `Runtime.enable`, `Page.enable`

**Key events/methods:**
- `Network.getAllCookies` → all cookies across all domains in the browser jar (richer than `getCookies`)
- `Runtime.evaluate` → localStorage, sessionStorage, indexedDB.databases(), caches.keys(), navigator.serviceWorker.getRegistrations(), navigator.storage.estimate()
- `Network.getCookies({ urls })` → scoped to a specific origin when full jar is too noisy

**Script skeleton:**

```js
export async function run(cdp) {
  await cdp.send('Network.enable', {});
  await cdp.send('Runtime.enable', {});
  await cdp.send('Page.enable', {});
  cdp.on('Page.javascriptDialogOpening', () =>
    cdp.send('Page.handleJavaScriptDialog', { accept: false }).catch(() => {}));

  await cdp.send('Page.navigate', { url: 'https://TARGET_URL/' });
  await new Promise(r => setTimeout(r, 5000)); // settle

  // ── Cookies (all domains) ─────────────────────────────────────────────────
  const { cookies } = await cdp.send('Network.getAllCookies', {});
  console.log(`[SECURITY] Cookies total: ${cookies.length}`);
  for (const c of cookies) {
    const flags = [c.httpOnly ? 'httpOnly' : 'NO_httpOnly', c.secure ? 'secure' : 'NO_secure', `sameSite=${c.sameSite || 'None'}`].join(' ');
    console.log(`[SECURITY] Cookie: ${c.name.padEnd(28)} domain=${c.domain} [${flags}]`);
    if (!c.httpOnly) console.log(`[FINDING] COOKIE_NO_HTTPONLY: ${c.name}`);
    if (!c.secure)   console.log(`[FINDING] COOKIE_NO_SECURE: ${c.name}`);
  }

  // ── localStorage ─────────────────────────────────────────────────────────
  const { result: lsR } = await cdp.send('Runtime.evaluate', {
    expression: `JSON.stringify(Object.entries(localStorage).map(([k,v])=>({k,size:new Blob([k+v]).size,sensitive:/token|auth|jwt|secret|password|credential/i.test(k)})))`,
    returnByValue: true,
  });
  const ls = JSON.parse(lsR.value || '[]');
  console.log(`[STORAGE] localStorage: ${ls.length} keys, ~${ls.reduce((s,i)=>s+i.size,0)}B`);
  for (const { k, size, sensitive } of ls) {
    console.log(`[STORAGE] LS[${k}] size=${size}B`);
    if (sensitive) console.log(`[FINDING] SENSITIVE_IN_STORAGE: localStorage["${k}"]`);
  }

  // ── sessionStorage ────────────────────────────────────────────────────────
  const { result: ssR } = await cdp.send('Runtime.evaluate', {
    expression: `JSON.stringify(Object.entries(sessionStorage).map(([k,v])=>({k,size:new Blob([k+v]).size,sensitive:/token|auth|jwt|secret|password|credential/i.test(k)})))`,
    returnByValue: true,
  });
  const ss = JSON.parse(ssR.value || '[]');
  console.log(`[STORAGE] sessionStorage: ${ss.length} keys`);
  for (const { k, size, sensitive } of ss) {
    console.log(`[STORAGE] SS[${k}] size=${size}B`);
    if (sensitive) console.log(`[FINDING] SENSITIVE_IN_STORAGE: sessionStorage["${k}"]`);
  }

  // ── IndexedDB ─────────────────────────────────────────────────────────────
  const { result: idbR } = await cdp.send('Runtime.evaluate', {
    expression: `indexedDB.databases().then(dbs=>JSON.stringify(dbs))`,
    returnByValue: true, awaitPromise: true,
  });
  const dbs = JSON.parse(idbR.value || '[]');
  console.log(`[STORAGE] IndexedDB: ${dbs.length} databases`);
  for (const db of dbs) console.log(`[STORAGE] IDB: ${db.name} v${db.version}`);

  // ── Service Workers ───────────────────────────────────────────────────────
  const { result: swR } = await cdp.send('Runtime.evaluate', {
    expression: `navigator.serviceWorker.getRegistrations().then(r=>JSON.stringify(r.map(s=>({scope:s.scope,state:s.active?.state??'none',script:s.active?.scriptURL}))))`,
    returnByValue: true, awaitPromise: true,
  });
  const sws = JSON.parse(swR.value || '[]');
  console.log(`[STORAGE] Service Workers: ${sws.length}`);
  for (const sw of sws) console.log(`[STORAGE] SW: ${sw.scope} state=${sw.state} script=${sw.script}`);

  // ── Cache Storage ─────────────────────────────────────────────────────────
  const { result: cacheR } = await cdp.send('Runtime.evaluate', {
    expression: `caches.keys().then(names=>Promise.all(names.map(n=>caches.open(n).then(c=>c.keys().then(k=>({name:n,count:k.length})))))).then(JSON.stringify)`,
    returnByValue: true, awaitPromise: true,
  });
  const cachesData = JSON.parse(cacheR.value || '[]');
  console.log(`[STORAGE] Cache Storage: ${cachesData.length} caches`);
  for (const c of cachesData) console.log(`[STORAGE] Cache "${c.name}": ${c.count} entries`);

  // ── Quota ─────────────────────────────────────────────────────────────────
  const { result: quotaR } = await cdp.send('Runtime.evaluate', {
    expression: `navigator.storage.estimate().then(e=>JSON.stringify({usedKB:Math.round(e.usage/1024),quotaMB:Math.round(e.quota/1024/1024),pct:((e.usage/e.quota)*100).toFixed(2)+'%'}))`,
    returnByValue: true, awaitPromise: true,
  });
  console.log(`[STORAGE] Quota: ${quotaR.value}`);

  // ── Cookie resurrection detection ─────────────────────────────────────────
  // Checks whether tracking IDs exist in both cookies AND localStorage (persistence pattern)
  const cookieNames = cookies.map(c => c.name);
  for (const { k } of ls) {
    if (cookieNames.includes(k))
      console.log(`[FINDING] COOKIE_RESURRECTION: "${k}" duplicated in both cookies and localStorage — tracking persistence pattern`);
  }
}
```

**Output prefixes:** `[STORAGE]` `[SECURITY]` `[FINDING]`

**Key findings to watch:**
- `COOKIE_NO_HTTPONLY` / `COOKIE_NO_SECURE` — tracking cookies readable by JS or transmittable over HTTP
- `SENSITIVE_IN_STORAGE` — token/auth/JWT keys in localStorage
- `COOKIE_RESURRECTION` — IDs stored in both cookies and localStorage (e.g. cross-domain tracker IDs)
- Large IndexedDB stores or Cache Storage entries indicate PWA / offline capability

**Combine with:** `security` (CSP, headers, POST body scan), `consent` (GDPR pre-grant check), `supply-chain` (third-party domain inventory).

## consent

**Trigger phrases:** "GDPR", "consent", "tracking", "privacy", "CMP", "Usercentrics", "OneTrust", "Cookiebot", "cookie banner", "ad consent", "is consent required", "what is tracked", "analytics opt-out", "adStorage", "pre-granted"

**Purpose:** Audit whether a Consent Management Platform (CMP) is present, whether consent is properly gated before trackers fire, and whether stored consent state matches what was granted. Works across Usercentrics, OneTrust, CookieYes, Cookiebot, TrustArc, and custom CMPs.

**Domains:** `Network.enable`, `Runtime.enable`, `Page.enable`

**Key signals:**
- Analytics and ad trackers firing before consent dialog appears → violation
- `ucData` / `_uetsid` / `_gcl_au` in localStorage with consent already granted → pre-grant
- `window.dataLayer` populated before consent → GTM firing too early
- `window.UC_UI` / `window.__tcfapi` / `window.Optanon` → CMP present
- Consent string in localStorage (`ucString` = Usercentrics, `eupubconsent-v2` = IAB TCF v2)

**Script skeleton:**

```js
export async function run(cdp) {
  await cdp.send('Network.enable', {});
  await cdp.send('Runtime.enable', {});
  await cdp.send('Page.enable', {});
  cdp.on('Page.javascriptDialogOpening', () =>
    cdp.send('Page.handleJavaScriptDialog', { accept: false }).catch(() => {}));

  // Track tracker requests BEFORE consent can fire
  const trackerHits = [];
  cdp.on('Network.requestWillBeSent', ({ request, timestamp }) => {
    const url = request.url;
    if (/googletagmanager|google-analytics|clarity\.ms|bat\.bing|fbq|meta\.net|doubleclick|twitter|ads-twitter/i.test(url))
      trackerHits.push({ url: url.substring(0, 100), ts: timestamp });
  });

  await cdp.send('Page.navigate', { url: 'https://TARGET_URL/' });
  await new Promise(r => setTimeout(r, 6000));

  // ── CMP detection ─────────────────────────────────────────────────────────
  const { result: cmpR } = await cdp.send('Runtime.evaluate', {
    expression: `JSON.stringify({
      usercentrics: !!window.UC_UI || !!window.usercentrics,
      onetrust:     !!window.OneTrust || !!window.Optanon,
      cookiebot:    !!window.CookieConsent || !!window.Cookiebot,
      trustArc:     !!window.truste,
      iabTCF:       typeof window.__tcfapi === 'function',
      gtmLoaded:    Array.isArray(window.dataLayer),
      dlEvents:     (window.dataLayer || []).length,
    })`,
    returnByValue: true,
  });
  const cmp = JSON.parse(cmpR.value || '{}');
  console.log(`[SECURITY] CMP detected: ${JSON.stringify(cmp)}`);
  if (!Object.values(cmp).slice(0,5).some(Boolean)) console.log('[FINDING] NO_CMP: no consent management platform detected');

  // ── Consent state in storage ───────────────────────────────────────────────
  const { result: csR } = await cdp.send('Runtime.evaluate', {
    expression: `JSON.stringify({
      ucGcm:       (() => { try { return JSON.parse(localStorage.getItem('ucData') || '{}').gcm || null; } catch { return null; } })(),
      ucString:    !!localStorage.getItem('ucString'),
      iabConsent:  !!localStorage.getItem('eupubconsent-v2'),
      otConsent:   !!localStorage.getItem('OptanonConsent'),
    })`,
    returnByValue: true,
  });
  const cs = JSON.parse(csR.value || '{}');
  if (cs.ucGcm) {
    const gcm = cs.ucGcm;
    console.log(`[SECURITY] Usercentrics GCM flags: ${JSON.stringify(gcm)}`);
    if (gcm.adStorage === 'granted')        console.log('[FINDING] CONSENT_PRE_GRANTED: adStorage=granted without user interaction');
    if (gcm.adPersonalization === 'granted') console.log('[FINDING] CONSENT_PRE_GRANTED: adPersonalization=granted');
    if (gcm.analyticsStorage === 'granted') console.log('[FINDING] CONSENT_PRE_GRANTED: analyticsStorage=granted');
  }
  if (cs.iabConsent) console.log('[SECURITY] IAB TCF v2 string present (value redacted)');
  if (cs.otConsent) console.log('[SECURITY] OneTrust consent string present (value redacted)');

  // ── Trackers that fired before any consent interaction ─────────────────────
  console.log(`[SECURITY] Tracker requests on cold load: ${trackerHits.length}`);
  for (const h of trackerHits) console.log(`[SECURITY] Tracker fired: ${h.url}`);
  if (trackerHits.length > 0) console.log(`[FINDING] TRACKERS_BEFORE_CONSENT: ${trackerHits.length} tracker requests fired before consent could be shown`);

  // ── dataLayer events (GTM) ────────────────────────────────────────────────
  const { result: dlR } = await cdp.send('Runtime.evaluate', {
    expression: `JSON.stringify((window.dataLayer||[]).slice(0,10).map(e=>e.event||JSON.stringify(e).slice(0,60)))`,
    returnByValue: true,
  });
  const dlEvents = JSON.parse(dlR.value || '[]');
  console.log(`[SECURITY] dataLayer events: ${dlEvents.join(', ')}`);
}
```

**Output prefixes:** `[SECURITY]` `[FINDING]`

**Key findings to watch:**
- `NO_CMP` — no consent platform detected; all tracking is ungated
- `CONSENT_PRE_GRANTED` — adStorage/adPersonalization/analyticsStorage pre-granted (GDPR risk in EU)
- `TRACKERS_BEFORE_CONSENT` — trackers fire on first load before banner can appear
- Large `dataLayer` event count suggests aggressive GTM firing

**Combine with:** `storage` (full cookie + localStorage inventory), `supply-chain` (enumerate all third-party domains loaded), `network` (capture timing of tracker requests relative to page load).
