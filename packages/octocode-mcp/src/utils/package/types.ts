export interface PackageSearchInput {
  ecosystem: 'npm';
  name: string;
  itemsPerPage?: number;
  /** 1-based result page; maps to the registry `from` offset (page-1)*itemsPerPage. */
  page?: number;
  /** Boolean verbosity preference from the tool schema. true enables extended metadata by default. */
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
  /** Package name (canonical; replaces internal `path`). */
  name: string;
  /** npm registry page URL, e.g. https://www.npmjs.com/package/react */
  npmUrl: string;
  repoUrl: string | null;
  /** @deprecated use `name` — kept for internal mapping only, omitted from output */
  path?: string;
  version: string;
  /** Only populated for exact-name lookups via `npm view`. */
  mainEntry?: string | null;
  /** Only populated for exact-name lookups via `npm view`. */
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
