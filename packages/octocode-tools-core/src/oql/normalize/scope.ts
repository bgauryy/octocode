/* ------------------------------- scope ---------------------------------- */
import { diagnostic } from '../diagnostics.js';
import type { OqlInputQuery, QueryScope, QuerySource } from '../types.js';
import { fail } from './shared.js';

export function normalizeScope(
  raw: OqlInputQuery,
  from: QuerySource | undefined
): QueryScope | undefined {
  const scope: QueryScope = { ...(raw.scope ?? {}) };

  // path sugar resolution
  const topPath = raw.path;
  const usesTopPathAsSource =
    from?.kind === 'local' &&
    !raw.from &&
    typeof raw.repo !== 'string' &&
    typeof raw.owner !== 'string';

  if (topPath !== undefined && !usesTopPathAsSource) {
    if (raw.scope && raw.scope.path !== undefined) {
      fail(
        diagnostic(
          'ambiguousSugar',
          'Both top-level `path` and `scope.path` provided; the path intent is ambiguous.',
          { queryPath: 'path' }
        )
      );
    }
    scope.path = topPath;
  }

  return Object.keys(scope).length > 0 ? scope : undefined;
}
