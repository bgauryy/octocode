import { searchNpmPackage, checkNpmDeprecation } from './npm.js';
import { searchPythonPackage } from './python.js';

export interface PackageSearchInput {
  ecosystem: 'npm' | 'python';
  name: string;
  searchLimit?: number;
  npmFetchMetadata?: boolean;
  pythonFetchMetadata?: boolean;
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

export interface PythonPackageResult {
  name: string;
  version: string;
  description: string | null;
  keywords: string[];
  repository: string | null;
  homepage?: string;
  author?: string;
  license?: string;
  lastPublished?: string;
  owner?: string;
  repo?: string;
}

export type PackageResult =
  | MinimalPackageResult
  | NpmPackageResult
  | PythonPackageResult;

export interface PackageSearchAPIResult {
  packages: PackageResult[];
  ecosystem: 'npm' | 'python';
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
  const fetchMetadata =
    query.ecosystem === 'npm'
      ? (query.npmFetchMetadata ?? false)
      : (query.pythonFetchMetadata ?? false);

  const searchLimit = query.searchLimit ?? 1;

  // Call cached API functions (caching is done at the API layer)
  if (query.ecosystem === 'npm') {
    const result = await searchNpmPackage(
      query.name,
      searchLimit,
      fetchMetadata
    );

    return result;
  } else {
    return searchPythonPackage(query.name, fetchMetadata);
  }
}

export { checkNpmDeprecation };
