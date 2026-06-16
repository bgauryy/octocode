export interface NpmSearchInput {
  name: string;
  itemsPerPage?: number;
  page?: number;
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

  source?: 'cli' | 'registry' | 'cdn' | 'web';

  mainEntry?: string | null;
  moduleEntry?: string | null;

  typeDefinitions?: string | null;
  packageType?: 'module' | 'commonjs' | 'types-only' | 'unknown';
  exports?: string[];
  bin?: string[];
  repositoryDirectory?: string;
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

export interface NpmSearchAPIResult {
  packages: PackageResult[];
  totalFound: number;
  rawResponseChars?: number;
}

export interface NpmSearchError {
  error: string;
  hints?: string[];
}

export interface DeprecationInfo {
  deprecated: boolean;
  message?: string;
}
