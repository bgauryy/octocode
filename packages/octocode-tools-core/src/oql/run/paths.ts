/**
 * Local-path relativization for result rows, and its inverse for continuations.
 *
 * The prefix stripped from absolute local result paths so rows are concise and
 * match what the raw tools return. When the query root is a DIRECTORY, paths are
 * relative to it (`/…/src/oql` + `/…/src/oql/x.ts` -> `x.ts`, `adapters/y.ts`) —
 * exactly what `localSearchCode`/`ls`/`find` emit. When the root is a single
 * FILE (content reads), strip its directory so the row shows the basename. A
 * GitHub/unresolved root has no on-disk shape, so fall back to the parent.
 *
 * The SAME base feeds `localFileSource` (the inverse), so a relativized row path
 * always round-trips back to the correct absolute file for next.* continuations.
 */
import path from 'node:path';
import { statSync } from 'node:fs';
import type { OqlQuery, OqlResultRow } from '../types.js';

export function relativizeBase(root: string): string {
  const abs = path.resolve(root);
  try {
    if (statSync(abs).isDirectory()) return `${abs}${path.sep}`;
  } catch {
    /* not on disk yet — treat as parent-relative */
  }
  return `${path.dirname(abs)}${path.sep}`;
}

export function queryLocalRoot(query: OqlQuery): string | undefined {
  return query.from?.kind === 'local'
    ? query.from.path
    : query.from?.kind === 'materialized'
      ? query.from.localPath
      : undefined;
}

/**
 * Relativize absolute local result paths to the query root (see relativizeBase),
 * keeping `search` aligned with the former grep/ls shortcuts and far less verbose. Provider
 * (GitHub) paths are already repo-relative and left untouched.
 */
export function relativizeResultPaths(
  query: OqlQuery,
  results: OqlResultRow[]
): void {
  const root = queryLocalRoot(query);
  if (!root) return;
  const prefix = relativizeBase(root);
  for (const row of results) {
    const p = (row as { path?: string }).path;
    if (typeof p === 'string' && p.startsWith(prefix)) {
      (row as { path: string }).path = p.slice(prefix.length);
    }
  }
}

/**
 * For local/materialized sources, return a builder that turns a relativized
 * row path back into an absolute-file `from` (relativization stripped exactly
 * `dirname(resolve(root)) + '/'`, so re-adding it round-trips).
 */
export function localFileSource(
  query: OqlQuery
): ((rowPath: string) => OqlQuery['from']) | undefined {
  const root = queryLocalRoot(query);
  if (!root) return undefined;
  // Same base as relativizeResultPaths, minus the trailing separator, so a
  // relativized row path re-joins to the exact absolute file (round-trip).
  const base = relativizeBase(root).slice(0, -path.sep.length);
  return (rowPath: string) => ({
    kind: 'local',
    path: path.isAbsolute(rowPath) ? rowPath : path.join(base, rowPath),
  });
}
