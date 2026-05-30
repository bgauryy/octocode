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
  const fetchMetadata = query.npmFetchMetadata ?? false;
  const searchLimit = query.searchLimit ?? 1;

  return searchNpmPackage(query.name, searchLimit, fetchMetadata);
}

export { checkNpmDeprecation };
