import { openAwarenessStore } from '@octocodeai/octocode-awareness/host';
import { openOctocodeDb as openExtensionStateDb } from '@octocodeai/agent-contracts/db';
import { isPersistentStorageEnabledForExtension } from '@octocodeai/config';
import { extensionStateDbPath } from '../extension-paths.js';
import { resolveAwarenessDatabase } from './awareness-context.js';

export const PERSISTENT_AWARENESS_DISABLED_MESSAGE =
  'Persistent storage is disabled (storage.mode=memory); Awareness state is unavailable. '
  + 'Set extension.storage.mode="persistent" in ~/.octocode/.octocoderc to enable.';

/** Assert the shared persistence boundary without opening or creating a store. */
export function assertPersistentAwarenessEnabled(): void {
  if (!isPersistentStorageEnabledForExtension()) throw new Error(PERSISTENT_AWARENESS_DISABLED_MESSAGE);
}

/** Open SQLite state only when the resolved extension storage policy permits machine persistence. */
export function openOctocodeDb(): ReturnType<typeof openExtensionStateDb> {
  if (!isPersistentStorageEnabledForExtension()) {
    throw new Error(
      'Persistent storage is disabled (storage.mode=memory); SQLite state is unavailable. '
      + 'Set extension.storage.mode="persistent" in ~/.octocode/.octocoderc to enable.',
    );
  }
  return openExtensionStateDb(extensionStateDbPath());
}

/** Open durable Awareness state only when the extension storage policy permits persistence. */
export function openPersistentAwareness(
  options: Parameters<typeof openAwarenessStore>[0],
): ReturnType<typeof openAwarenessStore> {
  assertPersistentAwarenessEnabled();
  return openAwarenessStore({ ...options, dbPath: options?.dbPath ?? resolveAwarenessDatabase(options?.workspace ?? process.cwd(), options?.scope) });
}
