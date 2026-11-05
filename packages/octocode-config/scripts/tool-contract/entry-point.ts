import { realpathSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const realOrSelf = (path: string): string => {
  try {
    return realpathSync(path);
  } catch {
    return resolve(path);
  }
};

/** Whether `argv1` launched the module at `moduleUrl`: argv[1] keeps a symlinked path while import.meta.url is the real path. */
export function isEntryPoint(argv1: string | undefined, moduleUrl: string): boolean {
  return argv1 !== undefined && realOrSelf(argv1) === realOrSelf(fileURLToPath(moduleUrl));
}
