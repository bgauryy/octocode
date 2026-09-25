import { openOctocodeDb as openExtensionStateDb } from '../contracts/db.js';
import { isPersistentStorageEnabledForExtension } from '@octocodeai/config';
import { extensionStateDbPath } from '../extension-paths.js';
/** Open SQLite state only when the extension permits durable storage. */
export function openOctocodeDb(): ReturnType<typeof openExtensionStateDb> {
  if (!isPersistentStorageEnabledForExtension()) throw new Error('Persistent storage is disabled (storage.mode=memory); SQLite state is unavailable.');
  return openExtensionStateDb(extensionStateDbPath());
}
