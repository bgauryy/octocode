import { searchNpmPackage } from './npm/npmDeprecation.js';
import type {
  NpmSearchAPIResult,
  NpmSearchError,
  NpmSearchInput,
} from './types.js';

export async function searchPackage(
  query: NpmSearchInput
): Promise<NpmSearchAPIResult | NpmSearchError> {
  const isExact = query.mode === 'exact';
  const limit = query.itemsPerPage ?? (isExact ? 1 : 10);
  const from = Math.max(0, ((query.page ?? 1) - 1) * limit);
  return searchNpmPackage(
    query.name,
    limit,
    false,
    from,
    query.mode,
    query.registry
  );
}
