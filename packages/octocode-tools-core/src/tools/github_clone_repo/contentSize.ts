import { existsSync, readdirSync, lstatSync } from 'node:fs';
import { join } from 'node:path';
import { getDirectorySizeBytes } from '../../shared/fs-utils.js';

/**
 * Size of a clone's checked-out working tree, excluding the top-level `.git`
 * directory. The shared `getDirectorySizeBytes` counts everything, so a sparse
 * blob:none checkout of a few KB still reports the ~1MB packed-git floor —
 * making `totalSize`/`rawResponse` reflect Git plumbing, not the content the
 * caller actually received. `.git` only lives at the clone root, so skipping it
 * there and reusing the shared walker for every other subtree is sufficient.
 */
export function getCheckedOutSizeBytes(targetPath: string): number {
  if (!existsSync(targetPath)) return 0;

  let entries: string[];
  try {
    entries = readdirSync(targetPath);
  } catch {
    return 0;
  }

  let total = 0;
  for (const entry of entries) {
    if (entry === '.git') continue;
    const fullPath = join(targetPath, entry);
    try {
      const st = lstatSync(fullPath);
      if (st.isSymbolicLink()) continue;
      if (st.isDirectory()) {
        total += getDirectorySizeBytes(fullPath);
      } else if (st.isFile()) {
        total += st.size;
      }
    } catch {
      void 0;
    }
  }

  return total;
}
