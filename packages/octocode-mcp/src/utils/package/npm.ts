import { executeNpmCommand } from '../exec/npm.js';
import { fetchWithRetries } from '../http/fetch.js';
import { generateCacheKey, withDataCache } from '../http/cache.js';
import { isCircuitOpen } from '../http/circuitBreaker.js';
import type {
  PackageSearchAPIResult,
  PackageSearchError,
  NpmPackageResult,
  DeprecationInfo,
} from './types.js';
import {
  NpmViewResultSchema,
  NpmRegistrySearchSchema,
  NpmDeprecationOutputSchema,
} from './schemas.js';
import { countSerializedChars } from '../response/charSavings.js';

const DEFAULT_NPM_REGISTRY = 'https://registry.npmjs.org';

let _cachedRegistryUrl: string | null = null;

export async function getNpmRegistryUrl(): Promise<string> {
  if (_cachedRegistryUrl) return _cachedRegistryUrl;

  try {
    const result = await executeNpmCommand(
      'config',
      ['get', 'registry', '--no-workspaces'],
      { timeout: 10000 }
    );
    if (!result.error && result.exitCode === 0) {
      const url = result.stdout.trim().replace(/\/+$/, '');
      if (url && url.startsWith('http')) {
        _cachedRegistryUrl = url;
        return url;
      }
    }
  } catch {
    void 0;
  }

  _cachedRegistryUrl = DEFAULT_NPM_REGISTRY;
  return DEFAULT_NPM_REGISTRY;
}

export function _resetNpmRegistryUrlCache(): void {
  _cachedRegistryUrl = null;
}

export async function checkNpmRegistryReachable(): Promise<boolean> {
  try {
    const registryUrl = await getNpmRegistryUrl();
    const f = globalThis.fetch;
    if (!f) return false;
    const res = await f(registryUrl, {
      method: 'HEAD',
      signal: AbortSignal.timeout(5000),
    });
    return res.ok;
  } catch {
    return false;
  }
}

interface NpmViewResult {
  name: string;
  version: string;
  repository?: string | { url?: string; type?: string };
  main?: string;
  types?: string;
  typings?: string;
  description?: string;
  keywords?: string[];
  license?: string | { type?: string };
  homepage?: string;
  author?: string | { name?: string; email?: string; url?: string };
  maintainers?: Array<{ name?: string; email?: string }>;
  engines?: Record<string, string>;
  dependencies?: Record<string, string>;
  devDependencies?: Record<string, string>;
  peerDependencies?: Record<string, string>;
  time?: {
    modified?: string;
    created?: string;
    [version: string]: string | undefined;
  };
}

interface NpmRegistrySearchItem {
  name: string | null | undefined;
  version: string | null | undefined;
  description?: string | null;
  links?: {
    npm?: string | null;
    homepage?: string | null;
    repository?: string | null;
  };
}

interface NpmCliSearchItem {
  name?: string;
  version?: string;
  description?: string;
  keywords?: string[];
  date?: string;
  links?: {
    npm?: string;
    homepage?: string;
    repository?: string;
    bugs?: string;
  };
  repository?: string | { url?: string; type?: string };
  score?: {
    final?: number;
    detail?: { quality?: number; popularity?: number; maintenance?: number };
  };
}

function cleanRepoUrl(url: string): string {
  return url.replace(/^git\+/, '').replace(/\.git$/, '');
}

const NPM_DOWNLOADS_API = 'https://api.npmjs.org/downloads/point/last-week';

function countRawPayloadChars(raw: unknown): number {
  return raw === undefined ? 0 : countSerializedChars(raw);
}

async function fetchWeeklyDownloads(
  packageName: string
): Promise<{ downloads?: number; rawResponseChars: number }> {
  try {
    const url = `${NPM_DOWNLOADS_API}/${encodeURIComponent(packageName)}`;
    const data = (await fetchWithRetries(url, {
      maxRetries: 0,
      initialDelayMs: 300,
      headers: { Accept: 'application/json' },
      signal: AbortSignal.timeout(8000),
      packageRegistry: 'npm',
    })) as { downloads?: number } | null;
    return {
      downloads:
        typeof data?.downloads === 'number' ? data.downloads : undefined,
      rawResponseChars: countRawPayloadChars(data),
    };
  } catch {
    return { rawResponseChars: 0 };
  }
}

function isExactPackageName(query: string): boolean {
  if (query.startsWith('@') && query.includes('/')) {
    return true;
  }
  if (query.includes(' ')) {
    return false;
  }
  return /^[a-z0-9][a-z0-9._-]*$/i.test(query);
}

function isNetworkFetchError(error: string | undefined): boolean {
  if (!error) return false;
  const lower = error.toLowerCase();
  return (
    lower.includes('fetch failed') ||
    lower.includes('failed to fetch') ||
    lower.includes('network') ||
    lower.includes('econnrefused') ||
    lower.includes('enotfound') ||
    lower.includes('etimedout') ||
    lower.includes('socket hang up') ||
    lower.includes('connect timeout') ||
    lower.includes('circuit open') ||
    lower.includes('circuit breaker')
  );
}

export function _packageNameToSearchKeywords(packageName: string): string {
  return packageName
    .replace(/^@/, '')
    .replace(/[/_-]/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

function parseRegistrySearchTotal(
  total: string | number | undefined,
  fallback: number
): number {
  if (typeof total === 'number' && Number.isFinite(total)) return total;
  if (typeof total === 'string') {
    const parsed = Number.parseInt(total, 10);
    if (Number.isFinite(parsed)) return parsed;
  }
  return fallback;
}

function mapToResult(
  data: NpmViewResult,
  includeExtendedMetadata: boolean = false,
  source: 'cli' | 'registry' = 'cli'
): NpmPackageResult {
  let repoUrl: string | null = null;
  if (data.repository) {
    if (typeof data.repository === 'string') {
      repoUrl = cleanRepoUrl(data.repository);
    } else if (data.repository.url) {
      repoUrl = cleanRepoUrl(data.repository.url);
    }
  }

  let lastPublished: string | undefined;
  if (data.time) {
    const versionTime = data.version ? data.time[data.version] : undefined;
    const timeStr = versionTime || data.time.modified;
    if (timeStr) {
      lastPublished = timeStr;
    }
  }

  const result: NpmPackageResult = {
    name: data.name,
    npmUrl: `https://www.npmjs.com/package/${encodeURIComponent(data.name)}`,
    repoUrl,
    version: data.version || 'latest',
    mainEntry: data.main || null,
    typeDefinitions: data.types || data.typings || null,
    lastPublished,
    source,
  };

  if (data.description) {
    result.description = data.description;
  }
  if (data.license) {
    result.license =
      typeof data.license === 'string' ? data.license : data.license.type;
  }

  if (includeExtendedMetadata) {
    if (data.author) {
      if (typeof data.author === 'string') {
        result.author = data.author;
      } else if (data.author.name) {
        result.author = data.author.name;
      }
    }

    if (data.keywords && data.keywords.length > 0) {
      result.keywords = data.keywords;
    }
    if (data.homepage) {
      result.homepage = data.homepage;
    }
    if (data.engines && Object.keys(data.engines).length > 0) {
      result.engines = data.engines;
    }
    if (data.dependencies && Object.keys(data.dependencies).length > 0) {
      result.dependencies = data.dependencies;
    }
    if (
      data.peerDependencies &&
      Object.keys(data.peerDependencies).length > 0
    ) {
      result.peerDependencies = data.peerDependencies;
    }
  }

  return result;
}

function getSearchItemRepoUrl(item: NpmCliSearchItem): string | null {
  if (typeof item.links?.repository === 'string') {
    return cleanRepoUrl(item.links.repository);
  }
  if (typeof item.repository === 'string') {
    return cleanRepoUrl(item.repository);
  }
  if (typeof item.repository?.url === 'string') {
    return cleanRepoUrl(item.repository.url);
  }
  return null;
}

function mapSearchItemToResult(
  item: NpmCliSearchItem
): NpmPackageResult | null {
  if (!item.name) return null;
  const npmUrl =
    (item.links?.npm ?? '') ||
    `https://www.npmjs.com/package/${encodeURIComponent(item.name)}`;
  const homepage = item.links?.homepage ?? undefined;
  const result: NpmPackageResult = {
    name: item.name,
    npmUrl,
    repoUrl: getSearchItemRepoUrl(item),
    version: item.version ?? 'unknown',
    source: 'cli',
    ...(item.description ? { description: item.description } : {}),
    ...(homepage ? { homepage } : {}),
    ...(item.keywords && item.keywords.length > 0
      ? { keywords: item.keywords }
      : {}),
  };
  return result;
}

function encodeRegistryPackageName(packageName: string): string {
  if (packageName.startsWith('@')) {
    return '@' + packageName.slice(1).replace('/', '%2F');
  }
  return packageName;
}

async function fetchLastPublished(
  packageName: string
): Promise<{ lastPublished?: string; rawResponseChars: number }> {
  try {
    const registryUrl = await getNpmRegistryUrl();
    const urlName = encodeRegistryPackageName(packageName);
    const url = `${registryUrl}/${urlName}`;
    const signal = AbortSignal.timeout(8000);

    try {
      const data = (await fetchWithRetries(url, {
        maxRetries: 0,
        initialDelayMs: 300,
        headers: { Accept: 'application/vnd.npm.install-v1+json' },
        signal,
        packageRegistry: 'npm',
      })) as { modified?: string } | null;
      const rawResponseChars = countRawPayloadChars(data);
      if (data?.modified)
        return { lastPublished: data.modified, rawResponseChars };
    } catch {
      void 0;
    }

    const data = (await fetchWithRetries(url, {
      maxRetries: 0,
      initialDelayMs: 300,
      headers: { Accept: 'application/json' },
      signal,
      packageRegistry: 'npm',
    })) as { time?: { modified?: string } } | null;
    return {
      lastPublished: data?.time?.modified || undefined,
      rawResponseChars: countRawPayloadChars(data),
    };
  } catch {
    return { rawResponseChars: 0 };
  }
}

type PackageDetailsLookupResult = {
  pkg: NpmPackageResult | null;
  errorDetail?: string;
  rawResponseChars: number;
};

async function enrichPackageDetails(
  packageName: string,
  pkg: NpmPackageResult,
  rawResponseChars: number
): Promise<PackageDetailsLookupResult> {
  const [downloadsResult, lastPublishedResult] = await Promise.all([
    fetchWeeklyDownloads(packageName),
    pkg.lastPublished
      ? Promise.resolve({ lastPublished: undefined, rawResponseChars: 0 })
      : fetchLastPublished(packageName),
  ]);
  if (downloadsResult.downloads !== undefined) {
    pkg.weeklyDownloads = downloadsResult.downloads;
  }
  if (lastPublishedResult.lastPublished && !pkg.lastPublished) {
    pkg.lastPublished = lastPublishedResult.lastPublished;
  }

  return {
    pkg,
    rawResponseChars:
      rawResponseChars +
      downloadsResult.rawResponseChars +
      lastPublishedResult.rawResponseChars,
  };
}

function isNotFoundMessage(message: string): boolean {
  const lower = message.toLowerCase();
  return (
    lower.includes('404') ||
    lower.includes('not found') ||
    lower.includes('e404')
  );
}

async function fetchPackageDetailsFromCli(
  packageName: string,
  includeExtendedMetadata: boolean
): Promise<PackageDetailsLookupResult> {
  try {
    const result = await executeNpmCommand('view', [packageName, '--json'], {
      timeout: 8000,
    });
    if (!result) return { pkg: null, rawResponseChars: 0 };
    if (result.error || result.exitCode !== 0) {
      const msg =
        result.error?.message ||
        result.stderr ||
        `npm exited ${result.exitCode}`;
      return {
        pkg: null,
        ...(isNotFoundMessage(msg) ? {} : { errorDetail: msg }),
        rawResponseChars: countRawPayloadChars(result.stdout),
      };
    }

    const output = result.stdout.trim();
    if (!output || output === 'undefined') {
      return { pkg: null, rawResponseChars: 0 };
    }

    let raw: unknown;
    try {
      raw = JSON.parse(output);
    } catch {
      return {
        pkg: null,
        errorDetail: 'Invalid npm view JSON output',
        rawResponseChars: output.length,
      };
    }

    const rawResponseChars = countRawPayloadChars(raw);
    const validation = NpmViewResultSchema.safeParse(raw);
    if (!validation.success) {
      return {
        pkg: null,
        errorDetail: 'Invalid npm view response format',
        rawResponseChars,
      };
    }

    const pkg = mapToResult(
      validation.data as NpmViewResult,
      includeExtendedMetadata,
      'cli'
    );
    return { pkg, rawResponseChars };
  } catch (error) {
    const msg = error instanceof Error ? error.message : String(error);
    return {
      pkg: null,
      ...(isNotFoundMessage(msg) ? {} : { errorDetail: msg }),
      rawResponseChars: 0,
    };
  }
}

async function fetchPackageDetailsFromRegistry(
  packageName: string,
  includeExtendedMetadata: boolean
): Promise<PackageDetailsLookupResult> {
  try {
    const registryUrl = await getNpmRegistryUrl();
    const urlName = encodeRegistryPackageName(packageName);
    const url = `${registryUrl}/${urlName}/latest`;

    let raw: unknown;
    try {
      raw = await fetchWithRetries(url, {
        maxRetries: 1,
        initialDelayMs: 500,
        headers: { Accept: 'application/json' },
        signal: AbortSignal.timeout(8000),
        packageRegistry: 'npm',
      });
    } catch (fetchErr) {
      const msg =
        fetchErr instanceof Error ? fetchErr.message : String(fetchErr);
      if (msg.includes('404') || msg.toLowerCase().includes('not found')) {
        return { pkg: null, rawResponseChars: 0 };
      }
      return { pkg: null, errorDetail: msg, rawResponseChars: 0 };
    }

    const rawResponseChars = countRawPayloadChars(raw);

    if (!raw || typeof raw !== 'object') {
      return { pkg: null, rawResponseChars };
    }

    const validation = NpmViewResultSchema.safeParse(raw);
    if (!validation.success) {
      return {
        pkg: null,
        errorDetail: 'Invalid npm registry response format',
        rawResponseChars,
      };
    }

    const pkg = mapToResult(
      validation.data as NpmViewResult,
      includeExtendedMetadata,
      'registry'
    );

    return { pkg, rawResponseChars };
  } catch (error) {
    const msg = error instanceof Error ? error.message : String(error);
    return {
      pkg: null,
      ...(isNotFoundMessage(msg) ? {} : { errorDetail: msg }),
      rawResponseChars: 0,
    };
  }
}

async function fetchPackageDetailsWithError(
  packageName: string,
  includeExtendedMetadata: boolean = false
): Promise<PackageDetailsLookupResult> {
  const [cliSettled, regSettled] = await Promise.allSettled([
    fetchPackageDetailsFromCli(packageName, includeExtendedMetadata),
    fetchPackageDetailsFromRegistry(packageName, includeExtendedMetadata),
  ]);

  /* c8 ignore next */
  const cliResult =
    cliSettled.status === 'fulfilled'
      ? cliSettled.value
      : { pkg: null, rawResponseChars: 0 };
  /* c8 ignore next */
  const regResult =
    regSettled.status === 'fulfilled'
      ? regSettled.value
      : { pkg: null, rawResponseChars: 0 };

  const winner = cliResult.pkg ? cliResult : regResult.pkg ? regResult : null;
  if (winner?.pkg) {
    return enrichPackageDetails(
      packageName,
      winner.pkg,
      winner.rawResponseChars
    );
  }

  const errorDetail =
    (cliResult as PackageDetailsLookupResult).errorDetail ||
    (regResult as PackageDetailsLookupResult).errorDetail;
  return {
    pkg: null,
    errorDetail,
    rawResponseChars: cliResult.rawResponseChars + regResult.rawResponseChars,
  };
}

async function fetchNpmPackageByView(
  packageName: string,
  fetchMetadata: boolean
): Promise<PackageSearchAPIResult | PackageSearchError> {
  const { pkg, errorDetail, rawResponseChars } =
    await fetchPackageDetailsWithError(packageName, fetchMetadata);

  if (!pkg) {
    if (errorDetail) {
      const isNetwork = isNetworkFetchError(errorDetail);
      return {
        error: `NPM view failed for '${packageName}': ${errorDetail}`,
        rawResponseChars,
        hints: isNetwork
          ? [
              'npm registry is unreachable.',
              'Use `githubSearchRepositories` to find the source repo directly by package name or domain terms.',
            ]
          : [
              'Ensure npm is installed and available in PATH',
              'Check package name for typos',
              `Try: npm view ${packageName} --json`,
            ],
      };
    }
    return {
      packages: [],
      totalFound: 0,
      rawResponseChars,
    };
  }

  return {
    packages: [pkg],
    totalFound: 1,
    rawResponseChars,
  };
}

async function searchNpmPackageViaCliSearch(
  keywords: string,
  limit: number,
  fetchMetadata: boolean,
  from: number = 0
): Promise<PackageSearchAPIResult | PackageSearchError> {
  const searchLimit = Math.max(limit + from, limit);
  const result = await executeNpmCommand(
    'search',
    [keywords, '--json', '--searchlimit', String(searchLimit)],
    { timeout: 8000 }
  );
  if (!result)
    return { error: 'NPM CLI search unavailable', rawResponseChars: 0 };
  if (result.error || result.exitCode !== 0) {
    const msg =
      result.error?.message || result.stderr || `npm exited ${result.exitCode}`;
    return {
      error: `NPM CLI search failed: ${msg}`,
      rawResponseChars: countRawPayloadChars(result.stdout),
    };
  }

  const output = result.stdout.trim();
  if (!output) {
    return {
      packages: [],
      totalFound: 0,
      rawResponseChars: 0,
    };
  }

  let raw: unknown;
  try {
    raw = JSON.parse(output);
  } catch {
    return {
      error: 'Invalid npm search JSON output',
      rawResponseChars: output.length,
    };
  }

  const rawResponseChars = countRawPayloadChars(raw);
  if (!Array.isArray(raw)) {
    return {
      error: 'Invalid npm search response format',
      rawResponseChars,
    };
  }

  const pageItems = raw.slice(from, from + limit) as unknown[];
  const packageResults = await Promise.all(
    pageItems.map(async item => {
      if (!item || typeof item !== 'object') return null;
      const searchItem = item as NpmCliSearchItem;
      if (!searchItem.name) return null;
      if (fetchMetadata) {
        const detailsResult = await fetchPackageDetailsWithError(
          searchItem.name,
          true
        );
        if (detailsResult.pkg) return detailsResult;
      }
      return {
        pkg: mapSearchItemToResult(searchItem),
        rawResponseChars: 0,
      };
    })
  );

  const packages = packageResults
    .map(item => item?.pkg)
    .filter((pkg): pkg is NpmPackageResult => Boolean(pkg));
  const detailRawResponseChars = packageResults.reduce(
    (sum, item) => sum + (item?.rawResponseChars ?? 0),
    0
  );

  return {
    packages,
    totalFound: raw.length,
    rawResponseChars: rawResponseChars + detailRawResponseChars,
  };
}

async function searchNpmPackageViaRegistrySearch(
  keywords: string,
  limit: number,
  fetchMetadata: boolean,
  from: number = 0
): Promise<PackageSearchAPIResult | PackageSearchError> {
  try {
    const fromParam = from > 0 ? `&from=${from}` : '';
    const url = `${DEFAULT_NPM_REGISTRY}/-/v1/search?text=${encodeURIComponent(keywords)}&size=${limit}${fromParam}`;

    let raw: unknown;
    try {
      raw = await fetchWithRetries(url, {
        maxRetries: 1,
        initialDelayMs: 500,
        signal: AbortSignal.timeout(8000),
        packageRegistry: 'npm',
      });
    } catch (fetchErr) {
      const msg =
        fetchErr instanceof Error ? fetchErr.message : String(fetchErr);
      return {
        error: `NPM registry search failed: ${msg}`,
        hints: [
          'Check package name for typos',
          'Try searching with a simpler term',
        ],
      };
    }

    const searchRawResponseChars = countRawPayloadChars(raw);

    if (!raw || typeof raw !== 'object') {
      return {
        packages: [],
        totalFound: 0,
        rawResponseChars: searchRawResponseChars,
      };
    }

    const validation = NpmRegistrySearchSchema.safeParse(raw);
    if (!validation.success) {
      const issues = validation.error.issues.map(i => i.message).join('; ');
      return {
        error: `Invalid npm registry search response format: ${issues}`,
        rawResponseChars: searchRawResponseChars,
        hints: [
          'Try a different search term',
          'Try itemsPerPage=1 for an exact package lookup',
        ],
      };
    }

    const searchResults = (
      validation.data.objects
        .map(obj => obj.package as NpmRegistrySearchItem)
        .filter(
          (pkg): pkg is NpmRegistrySearchItem & { name: string } =>
            typeof pkg.name === 'string' && pkg.name.length > 0
        ) as (NpmRegistrySearchItem & { name: string })[]
    ).slice(0, limit);

    const packageResults = await Promise.all(
      searchResults.map(async item => {
        if (fetchMetadata) {
          const detailsResult = await fetchPackageDetailsWithError(
            item.name,
            true
          );
          if (detailsResult.pkg) return detailsResult;
        }

        return {
          pkg: {
            name: item.name,
            npmUrl:
              (item.links?.npm ?? '') ||
              `https://www.npmjs.com/package/${encodeURIComponent(item.name)}`,
            repoUrl:
              item.links?.repository &&
              typeof item.links.repository === 'string'
                ? cleanRepoUrl(item.links.repository)
                : null,
            version: item.version ?? 'unknown',
            source: 'registry' as const,
            ...(item.description
              ? { description: item.description as string }
              : {}),
            ...(item.links?.homepage
              ? { homepage: item.links.homepage as string }
              : {}),
          } as NpmPackageResult,
          rawResponseChars: 0,
        };
      })
    );
    const packages = packageResults
      .map(result => result.pkg)
      .filter((pkg): pkg is NpmPackageResult => Boolean(pkg));
    const detailRawResponseChars = packageResults.reduce(
      (sum, result) => sum + result.rawResponseChars,
      0
    );

    return {
      packages,
      totalFound: parseRegistrySearchTotal(
        validation.data.total,
        packages.length
      ),
      rawResponseChars: searchRawResponseChars + detailRawResponseChars,
    };
    /* c8 ignore next */
  } catch (error) {
    const errorMsg = error instanceof Error ? error.message : String(error);
    return {
      error: `NPM registry search failed: ${errorMsg}`,
      hints: [
        'Check package name for typos',
        'Try searching with a simpler term',
        'Ensure npm registry is accessible',
      ],
    };
  }
}

async function searchNpmPackageViaWebSearch(
  packageName: string,
  limit: number
): Promise<PackageSearchAPIResult | PackageSearchError> {
  const NPMS_API = 'https://api.npms.io/v2/search';
  const USER_AGENT =
    'octocode-mcp/1.0 (+https://github.com/bgauryy/octocode-mcp)';

  try {
    const url = `${NPMS_API}?q=${encodeURIComponent(packageName)}&size=${limit}`;
    let raw: unknown;
    try {
      raw = await fetchWithRetries(url, {
        maxRetries: 1,
        initialDelayMs: 500,
        headers: {
          Accept: 'application/json',
          'User-Agent': USER_AGENT,
        },
        signal: AbortSignal.timeout(8000),
      });
    } catch (fetchErr) {
      const msg =
        fetchErr instanceof Error ? fetchErr.message : String(fetchErr);
      return { error: `Web search failed: ${msg}` };
    }

    if (!raw || typeof raw !== 'object') {
      return {
        packages: [],
        totalFound: 0,
        rawResponseChars: countRawPayloadChars(raw),
      };
    }

    const data = raw as {
      results?: Array<{
        package?: {
          name?: string;
          version?: string;
          description?: string;
          links?: {
            npm?: string;
            repository?: string;
            homepage?: string;
          };
        };
      }>;
      total?: number;
    };

    if (!Array.isArray(data.results)) {
      return {
        packages: [],
        totalFound: 0,
        rawResponseChars: countRawPayloadChars(raw),
      };
    }

    const packages: NpmPackageResult[] = data.results
      .slice(0, limit)
      .map(item => item.package)
      .filter(
        (pkg): pkg is NonNullable<typeof pkg> & { name: string } =>
          typeof pkg?.name === 'string' && pkg.name.length > 0
      )
      .map(pkg => ({
        name: pkg.name,
        npmUrl:
          pkg.links?.npm ??
          `https://www.npmjs.com/package/${encodeURIComponent(pkg.name)}`,
        repoUrl:
          pkg.links?.repository && typeof pkg.links.repository === 'string'
            ? cleanRepoUrl(pkg.links.repository)
            : null,
        version: pkg.version ?? 'unknown',
        source: 'web' as const,
        ...(pkg.description ? { description: pkg.description } : {}),
        ...(pkg.links?.homepage ? { homepage: pkg.links.homepage } : {}),
      }));

    return {
      packages,
      totalFound: typeof data.total === 'number' ? data.total : packages.length,
      rawResponseChars: countRawPayloadChars(raw),
    };
    /* c8 ignore next */
  } catch (error) {
    const msg = error instanceof Error ? error.message : String(error);
    return { error: `Web search failed: ${msg}` };
  }
}

async function searchNpmPackageViaSearch(
  keywords: string,
  limit: number,
  fetchMetadata: boolean,
  from: number = 0
): Promise<PackageSearchAPIResult | PackageSearchError> {
  try {
    const cliResult = await searchNpmPackageViaCliSearch(
      keywords,
      limit,
      fetchMetadata,
      from
    );
    if (!('error' in cliResult)) return cliResult;
  } catch {
    void 0;
  }

  return searchNpmPackageViaRegistrySearch(
    keywords,
    limit,
    fetchMetadata,
    from
  );
}

async function enrichTopSearchResult(
  result: PackageSearchAPIResult
): Promise<PackageSearchAPIResult> {
  /* c8 ignore next */
  if (result.packages.length === 0) return result;
  const topPkg = result.packages[0] as NpmPackageResult;
  if (topPkg.weeklyDownloads !== undefined && topPkg.lastPublished)
    return result;
  const [downloadsResult, lastPublishedResult] = await Promise.all([
    /* c8 ignore next */
    topPkg.weeklyDownloads !== undefined
      ? Promise.resolve({ downloads: undefined, rawResponseChars: 0 })
      : fetchWeeklyDownloads(topPkg.name),
    /* c8 ignore next */
    topPkg.lastPublished
      ? Promise.resolve({ lastPublished: undefined, rawResponseChars: 0 })
      : fetchLastPublished(topPkg.name),
  ]);
  const enriched: NpmPackageResult = { ...topPkg };
  if (downloadsResult.downloads !== undefined) {
    enriched.weeklyDownloads = downloadsResult.downloads;
  }
  if (lastPublishedResult.lastPublished) {
    enriched.lastPublished = lastPublishedResult.lastPublished;
  }
  return { ...result, packages: [enriched, ...result.packages.slice(1)] };
}

export async function searchNpmPackage(
  packageName: string,
  limit: number,
  fetchMetadata: boolean,
  from: number = 0
): Promise<PackageSearchAPIResult | PackageSearchError> {
  const cacheKey = generateCacheKey('npm-search', {
    name: packageName,
    limit,
    metadata: fetchMetadata,
    from,
  });

  return withDataCache(
    cacheKey,
    async () => {
      const registryCircuitOpen = isCircuitOpen(DEFAULT_NPM_REGISTRY);

      if (registryCircuitOpen) {
        const webResult = await searchNpmPackageViaWebSearch(
          packageName,
          limit
        );
        if (!('error' in webResult) && webResult.packages.length > 0) {
          return webResult;
        }
        return {
          error:
            'npm registry circuit open and web search returned no results.',
          hints: [
            'Use `githubSearchRepositories` to find the source repo directly.',
          ],
        } as PackageSearchError;
      }

      if (from === 0 && isExactPackageName(packageName)) {
        const exactResult = await fetchNpmPackageByView(
          packageName,
          fetchMetadata
        );
        if ('error' in exactResult) {
          if (!isNetworkFetchError(exactResult.error)) return exactResult;
          /* c8 ignore next */
        } else if (exactResult.packages.length > 0 || limit === 1) {
          return exactResult;
        }
      }
      const searchResult = await searchNpmPackageViaSearch(
        packageName,
        limit,
        fetchMetadata,
        from
      );
      if (!('error' in searchResult) && searchResult.packages.length > 0) {
        return enrichTopSearchResult(searchResult);
      }
      const keywords = _packageNameToSearchKeywords(packageName);
      if (keywords !== packageName) {
        const kwResult = await searchNpmPackageViaSearch(
          keywords,
          limit,
          fetchMetadata,
          from
        );
        if (!('error' in kwResult) && kwResult.packages.length > 0) {
          return enrichTopSearchResult(kwResult);
        }
      }
      const webResult = await searchNpmPackageViaWebSearch(packageName, limit);
      if (!('error' in webResult) && webResult.packages.length > 0) {
        return webResult;
      }
      if ('error' in searchResult) {
        return {
          ...searchResult,
          hints: [
            'npm registry and web search (npms.io) are both unreachable.',
            'Use `githubSearchRepositories` to find the source repo directly by package name or domain terms.',
          ],
        };
      }
      return searchResult;
    },
    {
      shouldCache: result => {
        if ('error' in result) return false;
        if ('totalFound' in result && result.totalFound === 0) return false;
        return true;
      },
    }
  );
}

export async function checkNpmDeprecation(
  packageName: string
): Promise<DeprecationInfo | null> {
  try {
    const result = await executeNpmCommand('view', [
      packageName,
      'deprecated',
      '--json',
    ]);

    if (result.error || result.exitCode !== 0) {
      return null;
    }

    const output = result.stdout.trim();

    if (!output || output === 'undefined' || output === '') {
      return { deprecated: false };
    }

    try {
      const raw = JSON.parse(output);
      const validation = NpmDeprecationOutputSchema.safeParse(raw);
      const message = validation.success ? validation.data : output;
      return {
        deprecated: true,
        message:
          typeof message === 'string' ? message : 'This package is deprecated',
      };
    } catch {
      return {
        deprecated: true,
        message: output,
      };
    }
  } catch {
    return null;
  }
}
