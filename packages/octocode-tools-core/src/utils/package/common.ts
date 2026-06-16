import {
  searchNpmPackage,
  checkNpmDeprecation,
  isExactPackageName,
} from './npm.js';
import type {
  NpmSearchAPIResult,
  NpmSearchError,
  NpmSearchInput,
} from './types.js';

export type {
  DeprecationInfo,
  MinimalPackageResult,
  NpmPackageResult,
  PackageResult,
  NpmSearchAPIResult,
  NpmSearchError,
  NpmSearchInput,
} from './types.js';

export async function searchPackage(
  query: NpmSearchInput
): Promise<NpmSearchAPIResult | NpmSearchError> {
  const isExact = isExactPackageName(query.name);
  const limit = query.itemsPerPage ?? (isExact ? 1 : 10);
  const from = Math.max(0, ((query.page ?? 1) - 1) * limit);
  return searchNpmPackage(query.name, limit, true, from);
}

export { checkNpmDeprecation };
