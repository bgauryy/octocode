import { searchNpmPackage, checkNpmDeprecation } from './npm.js';

export interface PackageSearchInput {
  ecosystem: 'npm';
  name: string;
  searchLimit?: number;
  npmFetchMetadata?: boolean;
  mainResearchGoal?: string;
  researchGoal?: string;
  reasoning?: string;
}

export interface MinimalPackageResult {
  name: string;
  repository: string | null;
  owner?: string;
  repo?: string;
}

export interface NpmPackageResult {
  repoUrl: string | null;
  path: string;
  version: string;
  mainEntry: string | null;
  typeDefinitions: string | null;
  lastPublished?: string;
  owner?: string;
  repo?: string;
  // Lightweight metadata (always included)
  description?: string;
  license?: string;
  weeklyDownloads?: number;
  // Extended metadata (available when npmFetchMetadata=true)
  keywords?: string[];
  homepage?: string;
  author?: string;
  engines?: Record<string, string>;
  dependencies?: Record<string, string>;
  peerDependencies?: Record<string, string>;
}

export type PackageResult = MinimalPackageResult | NpmPackageResult;

export interface PackageSearchAPIResult {
  packages: PackageResult[];
  ecosystem: 'npm';
  totalFound: number;
  rawResponseChars?: number;
}

export interface PackageSearchError {
  error: string;
  hints?: string[];
}

export interface DeprecationInfo {
  deprecated: boolean;
  message?: string;
}

export async function searchPackage(
  query: PackageSearchInput
): Promise<PackageSearchAPIResult | PackageSearchError> {
  const fetchMetadata = query.npmFetchMetadata ?? false;
  const searchLimit = query.searchLimit ?? 1;

  return searchNpmPackage(query.name, searchLimit, fetchMetadata);
}

export { checkNpmDeprecation };
