// Source: skills/common/scripts/lib/entrypoint.mjs; skill copies are generated.
import { realpathSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const realOrSelf = file => {
  try { return realpathSync(file); } catch { return resolve(file); }
};

// Installed skills may be symlinks: argv retains the link while import.meta.url resolves it.
export function isMainModule(moduleUrl) {
  return Boolean(process.argv[1]) &&
    realOrSelf(process.argv[1]) === realOrSelf(fileURLToPath(moduleUrl));
}
