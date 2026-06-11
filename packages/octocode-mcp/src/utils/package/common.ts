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
  PackageSearchMode,
} from './types.js';

function shouldFetchMetadata(
  query: PackageSearchInput,
  isExact: boolean
): boolean {
  if (query.mode === 'full') return true;
  if (query.mode === 'lean') return false;
  if (query.mode === 'smart') return isExact;
  return isExact;
}

export async function searchPackage(
  query: PackageSearchInput
): Promise<PackageSearchAPIResult | PackageSearchError> {
  // Exact names resolve one canonical package with full metadata;
  // keyword queries return a ranked lean list (npm-search page of 20) —
  // per-item metadata enrichment is opt-in via mode:"full".
  const isExact = isExactPackageName(query.name);
  const fetchMetadata = shouldFetchMetadata(query, isExact);
  const searchLimit = query.itemsPerPage ?? (isExact ? 1 : 20);
  const from = Math.max(0, ((query.page ?? 1) - 1) * searchLimit);

  return searchNpmPackage(query.name, searchLimit, fetchMetadata, from);
}

export { checkNpmDeprecation };
