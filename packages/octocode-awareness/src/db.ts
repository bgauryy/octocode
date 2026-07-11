/** Public compatibility barrel for db.ts. */
export { AWARENESS_APPLICATION_ID, DatabaseSync, memoryHome, resolveDbPath, connectDb, checkpointWal, connectCachedDb, getDb, getDeliveryFingerprint, setDeliveryFingerprint } from './db-runtime.js';
export { initDb } from './db-init.js';
export { tableColumns } from './db-introspection.js';
export { hasFts, ftsTermsForRow, rebuildFts, referenceKind, replaceMemoryReferences, evictExpiredLocks } from './db-search.js';
export type { DeliveryFingerprintKey } from './db-runtime.js';
export type { EvictExpiredLocksResult } from './db-search.js';
