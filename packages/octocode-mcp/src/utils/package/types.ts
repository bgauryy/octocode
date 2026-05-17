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
