export interface PackageSearchInput {
  name: string;
  itemsPerPage?: number;

  page?: number;

  verbose?: boolean;
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
  name: string;

  npmUrl: string;
  repoUrl: string | null;
  path?: string;
  version: string;

  source?: 'cli' | 'registry' | 'web';

  mainEntry?: string | null;

  typeDefinitions?: string | null;
  lastPublished?: string;
  owner?: string;
  repo?: string;
  description?: string;
  license?: string;
  weeklyDownloads?: number;
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
