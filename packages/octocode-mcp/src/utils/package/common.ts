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
  // Default to true so all results include description, license, weeklyDownloads,
  // mainEntry and typeDefinitions — fields agents need for package evaluation.
  const fetchMetadata = query.npmFetchMetadata ?? true;
  const searchLimit = query.itemsPerPage ?? 1;
  // Result-count cursor: page N fetches the registry window at offset
  // (N-1)*itemsPerPage, so matches beyond the first page are reachable.
  const from = Math.max(0, ((query.page ?? 1) - 1) * searchLimit);

  return searchNpmPackage(query.name, searchLimit, fetchMetadata, from);
}

export { checkNpmDeprecation };
