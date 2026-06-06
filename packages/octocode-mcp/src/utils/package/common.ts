import { searchNpmPackage, checkNpmDeprecation } from './npm.js';
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
  const fetchMetadata = query.npmFetchMetadata ?? query.verbose === true;
  const searchLimit = query.itemsPerPage ?? 1;
  const from = Math.max(0, ((query.page ?? 1) - 1) * searchLimit);

  return searchNpmPackage(query.name, searchLimit, fetchMetadata, from);
}

export { checkNpmDeprecation };
