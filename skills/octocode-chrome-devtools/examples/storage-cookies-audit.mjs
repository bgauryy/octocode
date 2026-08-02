import { writeFileSync } from 'fs';
import { join } from 'path';

export async function run(cdp) {
  await cdp.send('Runtime.enable');
  await cdp.send('Network.enable');
  let cookies = [];
  try { cookies = (await cdp.send('Network.getAllCookies')).cookies || []; } catch {}
  const storage = await cdp.send('Runtime.evaluate', {
    awaitPromise: true,
    returnByValue: true,
    expression: `(async () => {
      const safeKeys = obj => { try { return Object.keys(obj); } catch { return []; } };
      let indexedDBDatabases = [];
      try { indexedDBDatabases = (await indexedDB.databases()).map(db => ({ name: db.name || null, version: db.version || null })); } catch {}
      let cacheNames = [];
      try { cacheNames = await caches.keys(); } catch {}
      let serviceWorkers = [];
      try { serviceWorkers = (await navigator.serviceWorker?.getRegistrations?.() || []).map(r => ({ scope: r.scope, active: Boolean(r.active), waiting: Boolean(r.waiting), installing: Boolean(r.installing) })); } catch {}
      return { url: location.href, localStorageKeys: safeKeys(localStorage), sessionStorageKeys: safeKeys(sessionStorage), indexedDBDatabases, cacheNames, serviceWorkers };
    })()`
  });
  const cookieRows = cookies.map(c => ({ name: c.name, domain: c.domain, path: c.path, expires: c.expires, size: c.size, httpOnly: c.httpOnly, secure: c.secure, sameSite: c.sameSite, priority: c.priority, sourceScheme: c.sourceScheme }));
  const payload = { url: cdp.targetInfo.url, cookies: { count: cookieRows.length, rows: cookieRows }, storage: storage.result?.value || {} };
  const artifact = join(cdp.outputDir, 'storage-cookies-audit.json');
  writeFileSync(artifact, `${JSON.stringify(payload, null, 2)}\n`, { mode: 0o600 });
  console.log(`[METRIC] COOKIES count=${cookieRows.length} domains=${new Set(cookieRows.map(c => c.domain)).size}`);
  console.log(`[METRIC] STORAGE localStorageKeys=${payload.storage.localStorageKeys?.length ?? 0} sessionStorageKeys=${payload.storage.sessionStorageKeys?.length ?? 0} indexedDB=${payload.storage.indexedDBDatabases?.length ?? 0} caches=${payload.storage.cacheNames?.length ?? 0} serviceWorkers=${payload.storage.serviceWorkers?.length ?? 0}`);
  for (const row of cookieRows.slice(0, 10)) console.log(`[COOKIE_META] ${JSON.stringify(row)}`);
  console.log(`[ARTIFACT] STORAGE_COOKIES ${artifact}`);
}
