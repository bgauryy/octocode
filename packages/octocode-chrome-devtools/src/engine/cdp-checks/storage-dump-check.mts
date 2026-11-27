/**
 * Storage dump: localStorage and sessionStorage, IndexedDB, CacheStorage, service workers and
 * cookies of the current page's origin. Values are redacted ({redacted,type,bytes}) unless
 * STORAGE_DUMP_INCLUDE_VALUES=1, and then they reach the saved 0600 file only, never stdout.
 */
import { chmodSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import type { Payload, Session } from '../cdp-connection.mjs';
import { errorMessage } from '../error-message.mjs';
import { dumpIndexedDb, type StorageScope } from './storage-idb.mjs';
import { shownValue } from './storage-values.mjs';

const env = process.env;
const includeValues = env.STORAGE_DUMP_INCLUDE_VALUES === '1';
const idbRecords = env.STORAGE_DUMP_IDB_RECORDS
  ? Number(env.STORAGE_DUMP_IDB_RECORDS)
  : 0;

const out = (line: string) => {
  // oxlint-disable-next-line eslint/no-console -- stdout is the check's output protocol
  console.log(line);
};

/** The first scope Chrome accepts for `call`: storage key (current API), then security origin. */
async function viaScope<T>(
  scopes: StorageScope[],
  call: (scope: StorageScope) => Promise<T>
): Promise<{ result: T; scope: StorageScope }> {
  let last: unknown;
  for (const scope of scopes) {
    try {
      return { result: await call(scope), scope };
    } catch (error) {
      last = error;
    }
  }
  throw last;
}

async function loadPage(cdp: Session, url: string) {
  await cdp.send('Page.enable');
  const loaded = new Promise<void>(resolve =>
    cdp.on('Page.loadEventFired', () => resolve())
  );
  const nav = await cdp.send('Page.navigate', { url });
  if (nav.errorText) throw new Error(`navigation failed: ${nav.errorText}`);
  await loaded;
}

async function cacheStorage(
  cdp: Session,
  scopes: StorageScope[]
): Promise<{ name: string; entries: Payload[] }[]> {
  const { result } = await viaScope(scopes, scope =>
    cdp.send('CacheStorage.requestCacheNames', scope)
  );
  const caches = [];
  for (const cache of result.caches ?? []) {
    const entries: Payload[] = [];
    for (let skip = 0; ;) {
      const page = await cdp.send('CacheStorage.requestEntries', {
        cacheId: cache.cacheId,
        skipCount: skip,
        pageSize: 500,
      });
      for (const entry of page.cacheDataEntries ?? [])
        entries.push({
          url: entry.requestURL,
          method: entry.requestMethod,
          status: entry.responseStatus,
          responseType: entry.responseType,
          requestHeaderNames: (entry.requestHeaders ?? []).map(
            (h: Payload) => h.name
          ),
          responseHeaderNames: (entry.responseHeaders ?? []).map(
            (h: Payload) => h.name
          ),
          ...(includeValues
            ? {
                requestHeaders: entry.requestHeaders,
                responseHeaders: entry.responseHeaders,
              }
            : {}),
        });
      skip += page.cacheDataEntries?.length ?? 0;
      if (!page.hasMore || !page.cacheDataEntries?.length) break;
    }
    caches.push({ name: cache.cacheName, entries });
  }
  return caches;
}

async function serviceWorkers(cdp: Session, origin: string) {
  const registrations = new Map<string, Payload>();
  const versions = new Map<string, Payload>();
  cdp.on('ServiceWorker.workerRegistrationUpdated', (p: Payload) => {
    for (const r of p.registrations ?? [])
      registrations.set(r.registrationId, r);
  });
  cdp.on('ServiceWorker.workerVersionUpdated', (p: Payload) => {
    for (const v of p.versions ?? []) versions.set(v.versionId, v);
  });
  await cdp.send('ServiceWorker.enable');
  // Chrome replays the existing registrations as events right after enable.
  await new Promise(r => setTimeout(r, 300));
  await cdp.send('ServiceWorker.disable').catch(() => {});
  return [...registrations.values()]
    .filter(r => !r.isDeleted && String(r.scopeURL).startsWith(origin))
    .map(r => ({
      scopeURL: r.scopeURL,
      registrationId: r.registrationId,
      versions: [...versions.values()]
        .filter(v => v.registrationId === r.registrationId)
        .map(v => ({
          scriptURL: v.scriptURL,
          status: v.status,
          runningStatus: v.runningStatus,
        })),
    }));
}

export async function run(cdp: Session) {
  if (!Number.isSafeInteger(idbRecords) || idbRecords < 0) {
    out(
      '[FINDING] STORAGE_DUMP_INVALID {"error":"STORAGE_DUMP_IDB_RECORDS must be a non-negative integer"}'
    );
    process.exitCode = 2;
    return;
  }
  if (env.STORAGE_DUMP_URL) await loadPage(cdp, env.STORAGE_DUMP_URL);

  const { frameTree } = await cdp.send('Page.getFrameTree');
  const frame = frameTree.frame;
  const pageUrl: string = frame.url;
  const origin: string = frame.securityOrigin || new URL(pageUrl).origin;
  if (!origin || origin === 'null' || origin === '://') {
    out(
      `[FINDING] STORAGE_DUMP_UNAVAILABLE ${JSON.stringify({ area: 'origin', error: `page ${pageUrl} has no storage origin; navigate to an http(s) page or pass STORAGE_DUMP_URL` })}`
    );
    process.exitCode = 1;
    return;
  }
  const scopes: StorageScope[] = [];
  try {
    const key = await cdp.send('Storage.getStorageKeyForFrame', {
      frameId: frame.id,
    });
    if (key.storageKey) scopes.push({ storageKey: key.storageKey });
  } catch {
    // Older Chrome: the security origin names the storage.
  }
  scopes.push({ securityOrigin: origin });

  const errors: { area: string; error: string }[] = [];
  const area = async <T,>(
    name: string,
    fallback: T,
    work: () => Promise<T>
  ) => {
    try {
      return await work();
    } catch (error) {
      errors.push({ area: name, error: errorMessage(error) });
      return fallback;
    }
  };

  const domStorage = async (isLocalStorage: boolean) => {
    await cdp.send('DOMStorage.enable');
    const { result } = await viaScope(scopes, scope =>
      cdp.send('DOMStorage.getDOMStorageItems', {
        storageId: { ...scope, isLocalStorage },
      })
    );
    return (result.entries ?? []).map(([key, value]: [string, string]) => ({
      key,
      value: shownValue(value, includeValues),
    }));
  };

  const localStorage = await area('localStorage', [], () => domStorage(true));
  const sessionStorage = await area('sessionStorage', [], () =>
    domStorage(false)
  );
  const indexedDb = await area('indexedDb', [], () =>
    viaScope(scopes, scope =>
      dumpIndexedDb(cdp, scope, idbRecords, includeValues)
    ).then(r => r.result)
  );
  const caches = await area('cacheStorage', [], () =>
    cacheStorage(cdp, scopes)
  );
  const workers = await area('serviceWorkers', [], () =>
    serviceWorkers(cdp, origin)
  );
  const cookies = await area('cookies', [], async () =>
    (
      (await cdp.send('Network.getCookies', { urls: [pageUrl] })).cookies ?? []
    ).map((c: Payload) => ({
      name: c.name,
      domain: c.domain,
      path: c.path,
      secure: c.secure,
      httpOnly: c.httpOnly,
      sameSite: c.sameSite,
      expires: c.expires,
      size: c.size,
      partitionKey: c.partitionKey ?? null,
      ...(includeValues ? { value: c.value } : {}),
    }))
  );

  const stores = indexedDb.flatMap((db: Payload) => db.objectStores);
  const payload = {
    url: pageUrl,
    origin,
    valuesRedacted: !includeValues,
    idbRecordsPerStore: idbRecords,
    complete: errors.length === 0,
    errors,
    localStorage,
    sessionStorage,
    indexedDb,
    cacheStorage: caches,
    serviceWorkers: workers,
    cookies,
    collectedAt: new Date().toISOString(),
  };
  const artifact = join(cdp.outputDir, 'storage-dump.json');
  writeFileSync(artifact, `${JSON.stringify(payload, null, 2)}\n`, {
    mode: 0o600,
  });
  chmodSync(artifact, 0o600);

  const count = (rows: Payload[], pick: (row: Payload) => number) =>
    rows.reduce((n, row) => n + pick(row), 0);
  out(
    `[METRIC] STORAGE_DUMP origin=${origin} localStorage=${localStorage.length} sessionStorage=${sessionStorage.length} idbDatabases=${indexedDb.length} idbStores=${stores.length} idbEntries=${count(stores, s => s.entries)} idbRecordsSaved=${count(stores, s => s.records?.length ?? 0)} caches=${caches.length} cacheEntries=${count(caches, c => c.entries.length)} serviceWorkers=${workers.length} cookies=${cookies.length} valuesRedacted=${!includeValues}`
  );
  if (includeValues)
    out(
      `[FINDING] SECRET_VALUES_SAVED ${JSON.stringify({ file: artifact, mode: '0600', note: 'storage and cookie values are in this file only, not in stdout' })}`
    );
  out(`[ARTIFACT] STORAGE_DUMP ${artifact}`);
  for (const error of errors)
    out(`[FINDING] STORAGE_DUMP_UNAVAILABLE ${JSON.stringify(error)}`);
  if (errors.length) process.exitCode = 1;
}
