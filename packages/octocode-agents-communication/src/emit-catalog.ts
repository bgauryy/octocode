import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { renderCatalog } from './catalog/index.ts';
import { root } from './entry-points.ts';

/** The generated command catalog, rendered from the Zod sources in src/catalog. */
export const catalogFile = join(root, 'assets/catalog.json');

/** Writes the catalog; with `check`, fails when the committed file differs instead (a hand edit or a source change not yet emitted). */
export function emitCatalog(check = false): void {
  const text = renderCatalog();
  if (!check) return writeFileSync(catalogFile, text);
  let current = '';
  try { current = readFileSync(catalogFile, 'utf8'); } catch { /* a missing file is stale */ }
  if (current !== text) throw Error('assets/catalog.json is not what src/catalog generates: edit the Zod sources in src/catalog and run `yarn catalog`, never the JSON');
}

if (import.meta.main) emitCatalog(process.argv.includes('--check'));
