/**
 * Smart storage measure: cookies (meta only), local/sessionStorage keys,
 * IndexedDB, Cache Storage, service workers + risk score.
 * Never prints cookie/token values.
 */
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';

const FIXTURE_ORIGIN = 'https://storage.test/measure';
const FIXTURE_HTML = `<!doctype html><html><body>storage-measure<script>
try {
  localStorage.setItem('theme','dark');
  localStorage.setItem('tracking_id','demo');
  sessionStorage.setItem('step','1');
  document.cookie = 'session_demo=1; path=/; SameSite=Lax';
  document.cookie = 'prefs=1; path=/; SameSite=Lax';
} catch (e) {}
</script></body></html>`;

export async function run(cdp) {
  await cdp.send('Runtime.enable');
  await cdp.send('Network.enable');
  await cdp.send('Page.enable');

  const existing = process.env.MEASURE_EXISTING === '1';
  const measureUrl = process.env.MEASURE_URL || null;
  const useFixture = !existing && !measureUrl;

  if (useFixture) {
    await cdp.send('Fetch.enable', {
      patterns: [{ urlPattern: '*storage.test/*', requestStage: 'Request' }],
    });
    cdp.on('Fetch.requestPaused', async ({ requestId }) => {
      await cdp.send('Fetch.fulfillRequest', {
        requestId,
        responseCode: 200,
        responsePhrase: 'OK',
        responseHeaders: [{ name: 'content-type', value: 'text/html; charset=utf-8' }],
        body: Buffer.from(FIXTURE_HTML).toString('base64'),
      });
    });
  }

  const navUrl = measureUrl || (useFixture ? FIXTURE_ORIGIN : null);
  if (navUrl) {
    const loaded = new Promise((resolve) => {
      const t = setTimeout(resolve, Number(process.env.STORAGE_WAIT_MS || 2500));
      cdp.on('Page.loadEventFired', () => {
        clearTimeout(t);
        resolve();
      });
    });
    await cdp.send('Page.navigate', { url: navUrl });
    await loaded;
    await new Promise(r => setTimeout(r, 300));
  }

  // Ensure hermetic keys exist even if page script raced.
  if (useFixture) {
    await cdp.send('Runtime.evaluate', {
      expression: `(() => {
        try {
          localStorage.setItem('theme', 'dark');
          localStorage.setItem('tracking_id', 'demo');
          sessionStorage.setItem('step', '1');
        } catch {}
        return true;
      })()`,
    });
  }

  let cookies = [];
  try {
    cookies = (await cdp.send('Network.getAllCookies')).cookies || [];
  } catch {}

  const storageEval = await cdp.send('Runtime.evaluate', {
    awaitPromise: true,
    returnByValue: true,
    expression: `(async () => {
      const out = {
        url: location.href,
        localStorageKeys: [],
        sessionStorageKeys: [],
        suspiciousLocalKeys: [],
        suspiciousSessionKeys: [],
        indexedDBDatabases: [],
        cacheNames: [],
        serviceWorkers: [],
        note: null,
      };
      const suspiciousKey = (k) => /token|secret|auth|password|session|jwt|api[_-]?key|tracking|userid|user_id|visitor/i.test(k);
      try {
        out.localStorageKeys = Object.keys(localStorage);
        out.suspiciousLocalKeys = out.localStorageKeys.filter(suspiciousKey);
      } catch (e) { out.note = (out.note || '') + 'localStorage:' + e.message + ';'; }
      try {
        out.sessionStorageKeys = Object.keys(sessionStorage);
        out.suspiciousSessionKeys = out.sessionStorageKeys.filter(suspiciousKey);
      } catch (e) { out.note = (out.note || '') + 'sessionStorage:' + e.message + ';'; }
      try {
        out.indexedDBDatabases = (await indexedDB.databases()).map(db => ({
          name: db.name || null,
          version: db.version || null,
        }));
      } catch (e) { out.note = (out.note || '') + 'idb:' + e.message + ';'; }
      try { out.cacheNames = await caches.keys(); }
      catch (e) { out.note = (out.note || '') + 'caches:' + e.message + ';'; }
      try {
        out.serviceWorkers = (await navigator.serviceWorker?.getRegistrations?.() || []).map(r => ({
          scope: r.scope,
          active: Boolean(r.active),
          waiting: Boolean(r.waiting),
          installing: Boolean(r.installing),
        }));
      } catch (e) { out.note = (out.note || '') + 'sw:' + e.message + ';'; }
      return out;
    })()`,
  });

  if (storageEval?.exceptionDetails) {
    console.log(`[FINDING] STORAGE_EVAL_ERROR ${storageEval.exceptionDetails.text || storageEval.exceptionDetails.exception?.description || 'unknown'}`);
  }
  const storage = storageEval?.result?.value && typeof storageEval.result.value === 'object'
    ? storageEval.result.value
    : {
        url: cdp.targetInfo?.url || null,
        localStorageKeys: [],
        sessionStorageKeys: [],
        suspiciousLocalKeys: [],
        suspiciousSessionKeys: [],
        indexedDBDatabases: [],
        cacheNames: [],
        serviceWorkers: [],
        note: 'evaluate-returned-empty',
      };
  const pageHost = (() => {
    try { return new URL(cdp.targetInfo?.url || storage.url || 'http://local').hostname; }
    catch { return ''; }
  })();

  const cookieRows = cookies.map(c => ({
    name: c.name,
    domain: c.domain,
    path: c.path,
    expires: c.expires,
    size: c.size,
    httpOnly: c.httpOnly,
    secure: c.secure,
    sameSite: c.sameSite,
    session: !c.expires || c.expires <= 0,
    thirdParty: pageHost ? !String(c.domain || '').includes(pageHost.replace(/^www\./, '')) : false,
  }));

  const insecure = cookieRows.filter(c => !c.secure && !c.session);
  const nonHttpOnlySessionish = cookieRows.filter(c => !c.httpOnly && /sess|auth|token|jwt/i.test(c.name));
  const thirdParty = cookieRows.filter(c => c.thirdParty);

  const findings = [];
  if (insecure.length) findings.push({ code: 'INSECURE_COOKIES', count: insecure.length });
  if (nonHttpOnlySessionish.length) findings.push({ code: 'SENSITIVE_COOKIE_NOT_HTTPONLY', count: nonHttpOnlySessionish.length, names: nonHttpOnlySessionish.map(c => c.name).slice(0, 10) });
  if (thirdParty.length) findings.push({ code: 'THIRD_PARTY_COOKIES', count: thirdParty.length });
  if ((storage.suspiciousLocalKeys || []).length) {
    findings.push({ code: 'SUSPICIOUS_LOCALSTORAGE_KEYS', keys: storage.suspiciousLocalKeys.slice(0, 10) });
  }
  if ((storage.suspiciousSessionKeys || []).length) {
    findings.push({ code: 'SUSPICIOUS_SESSIONSTORAGE_KEYS', keys: storage.suspiciousSessionKeys.slice(0, 10) });
  }

  let health = 100;
  health -= Math.min(30, insecure.length * 10);
  health -= Math.min(30, nonHttpOnlySessionish.length * 15);
  health -= Math.min(20, (storage.suspiciousLocalKeys?.length || 0) * 10);
  health -= Math.min(15, thirdParty.length * 3);
  health = Math.max(0, Math.round(health));

  const payload = {
    url: cdp.targetInfo?.url || storage.url || null,
    cookies: {
      count: cookieRows.length,
      domains: [...new Set(cookieRows.map(c => c.domain))],
      rows: cookieRows,
    },
    storage,
    score: { health, findings },
    collectedAt: new Date().toISOString(),
  };

  const artifact = join(cdp.outputDir, 'storage-measure.json');
  writeFileSync(artifact, `${JSON.stringify(payload, null, 2)}\n`, { mode: 0o600 });

  console.log(`[METRIC] STORAGE health=${health} cookies=${cookieRows.length} local=${storage.localStorageKeys?.length ?? 0} session=${storage.sessionStorageKeys?.length ?? 0} idb=${storage.indexedDBDatabases?.length ?? 0} caches=${storage.cacheNames?.length ?? 0} sw=${storage.serviceWorkers?.length ?? 0}`);
  for (const f of findings.slice(0, 8)) console.log(`[FINDING] STORAGE_${f.code} ${JSON.stringify(f)}`);
  for (const row of cookieRows.slice(0, 10)) console.log(`[COOKIE_META] ${JSON.stringify(row)}`);
  console.log(`[ARTIFACT] STORAGE_MEASURE ${artifact}`);
}
