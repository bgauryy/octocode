// IndexedDB inventory for the storage-dump recipe: databases, object stores, key paths,
// indexes, exact entry counts, and records only when the caller asked for a per-store number.
import type { Payload, Session } from '../cdp-connection.mjs';
import { keyPathOf, shownValue } from './storage-values.mjs';

/** The CDP parameters that name an origin's storage: tried in order until one is accepted. */
export type StorageScope = Payload;

type RemoteObject = {
  value?: unknown;
  objectId?: string;
  description?: string;
};

/** A remote value as plain data: primitives directly, objects by value, else their description. */
async function plain(cdp: Session, remote?: RemoteObject): Promise<unknown> {
  if (!remote) return null;
  if ('value' in remote) return remote.value;
  if (remote.objectId) {
    try {
      const out = await cdp.send('Runtime.callFunctionOn', {
        objectId: remote.objectId,
        functionDeclaration: 'function(){return this}',
        returnByValue: true,
      });
      if (out.result && 'value' in out.result) return out.result.value;
    } catch {
      // Fall through to the description.
    }
  }
  return remote.description ?? null;
}

/** Entries in one store: Chrome's exact metadata count, else a page-by-page walk. */
async function countEntries(
  cdp: Session,
  scope: StorageScope,
  databaseName: string,
  objectStoreName: string
): Promise<number> {
  try {
    const meta = await cdp.send('IndexedDB.getMetadata', {
      ...scope,
      databaseName,
      objectStoreName,
    });
    if (Number.isSafeInteger(meta.entriesCount)) return meta.entriesCount;
  } catch {
    // The walk below counts the same entries.
  }
  let skip = 0;
  for (;;) {
    const page = await cdp.send('IndexedDB.requestData', {
      ...scope,
      databaseName,
      objectStoreName,
      skipCount: skip,
      pageSize: 1000,
    });
    skip += page.objectStoreDataEntries?.length ?? 0;
    if (!page.hasMore) return skip;
  }
}

/** The first `limit` records of a store in key order, values shown per `includeValues`. */
async function firstRecords(
  cdp: Session,
  scope: StorageScope,
  databaseName: string,
  objectStoreName: string,
  limit: number,
  includeValues: boolean
) {
  const page = await cdp.send('IndexedDB.requestData', {
    ...scope,
    databaseName,
    objectStoreName,
    skipCount: 0,
    pageSize: limit,
  });
  const rows = [];
  for (const entry of (page.objectStoreDataEntries ?? []).slice(0, limit))
    rows.push({
      key: await plain(cdp, entry.key),
      value: shownValue(await plain(cdp, entry.value), includeValues),
    });
  return rows;
}

/** Every database of the origin with its stores; `records` is the per-store record count. */
export async function dumpIndexedDb(
  cdp: Session,
  scope: StorageScope,
  records: number,
  includeValues: boolean
) {
  await cdp.send('IndexedDB.enable');
  const { databaseNames = [] } = await cdp.send(
    'IndexedDB.requestDatabaseNames',
    scope
  );
  const databases = [];
  for (const databaseName of databaseNames as string[]) {
    const { databaseWithObjectStores: db } = await cdp.send(
      'IndexedDB.requestDatabase',
      { ...scope, databaseName }
    );
    const stores = [];
    for (const store of db.objectStores ?? []) {
      stores.push({
        name: store.name,
        keyPath: keyPathOf(store.keyPath),
        autoIncrement: Boolean(store.autoIncrement),
        indexes: (store.indexes ?? []).map((index: Payload) => ({
          name: index.name,
          keyPath: keyPathOf(index.keyPath),
          unique: Boolean(index.unique),
          multiEntry: Boolean(index.multiEntry),
        })),
        entries: await countEntries(cdp, scope, databaseName, store.name),
        ...(records > 0
          ? {
              records: await firstRecords(
                cdp,
                scope,
                databaseName,
                store.name,
                records,
                includeValues
              ),
            }
          : {}),
      });
    }
    databases.push({
      name: db.name,
      version: db.version,
      objectStores: stores,
    });
  }
  return databases;
}
