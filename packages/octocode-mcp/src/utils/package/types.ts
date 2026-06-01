export interface PackageSearchInput {
  ecosystem: 'npm';
  name: string;
  itemsPerPage?: number;
  /** 1-based result page; maps to the registry `from` offset (page-1)*itemsPerPage. */
  page?: number;
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
