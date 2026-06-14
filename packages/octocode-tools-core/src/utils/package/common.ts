import {
  searchNpmPackage,
  checkNpmDeprecation,
  isExactPackageName,
} from './npm.js';
import type {
  PackageSearchAPIResult,
  PackageSearchError,
  PackageSearchInput,
} from './types.js';

export type {
  DeprecationInfo,
  MinimalPackageResult,
  NpmPackageResult,
  PackageResult,
  PackageSearchAPIResult,
  PackageSearchError,
  PackageSearchInput,
} from './types.js';

export async function searchPackage(
  query: PackageSearchInput
): Promise<PackageSearchAPIResult | PackageSearchError> {
  const isExact = isExactPackageName(query.name);
  // Always fetch full metadata so repositoryDirectory (sourceRoot) is populated.
  // Keyword search: cap at 10 so 10 parallel npm-view calls stay fast.
  const limit = query.itemsPerPage ?? (isExact ? 1 : 10);
  const from = Math.max(0, ((query.page ?? 1) - 1) * limit);
  return searchNpmPackage(query.name, limit, true, from);
}

export { checkNpmDeprecation };
