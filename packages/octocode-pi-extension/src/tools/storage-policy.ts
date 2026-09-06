import { openAwareness } from '@octocodeai/octocode-awareness';
import { openOctocodeDb as openExtensionStateDb } from '@octocodeai/octocode-awareness/mcp-state';
import { isPersistentStorageEnabled } from '@octocodeai/config';
import { extensionStateDbPath } from '../extension-paths.js';

/** Open SQLite state only when the resolved policy permits machine persistence. */
export function openOctocodeDb(): ReturnType<typeof openExtensionStateDb> {
  if (!isPersistentStorageEnabled()) {
    throw new Error(
      'Persistent storage is disabled (storage.mode=memory); SQLite state is unavailable',
    );
  }
  return openExtensionStateDb(extensionStateDbPath());
}

/** Open durable Awareness state only when machine persistence is enabled. */
export function openPersistentAwareness(
  options: Parameters<typeof openAwareness>[0],
): ReturnType<typeof openAwareness> {
  if (!isPersistentStorageEnabled()) {
    throw new Error(
      'Persistent storage is disabled (storage.mode=memory); Awareness state is unavailable',
    );
  }
  return openAwareness(options);
}
