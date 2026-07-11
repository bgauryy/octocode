import { executeNpmCommand } from '../../exec/npm.js';
import { fetchWithRetries } from '../../http/fetch.js';
import type {
  NpmPackageResult,
  NpmSearchAPIResult,
  NpmSearchError,
} from '../types.js';
import { NpmRegistrySearchSchema } from '../schemas.js';
import {
  DEFAULT_NPM_REGISTRY,
  type NpmCliSearchItem,
  type NpmRegistrySearchItem,
} from './npmRegistry.js';
import {
  cleanRepoUrl,
  countRawPayloadChars,
  fetchWeeklyDownloads,
  mapSearchItemToResult,
  parseRegistrySearchTotal,
} from './npmMappers.js';
import {
  fetchLastPublished,
  fetchPackageDetailsWithError,
} from './npmDetailsFetchers.js';

export async function searchNpmPackageViaCliSearch(
  keywords: string,
  limit: number,
  fetchMetadata: boolean,
  from: number = 0
): Promise<NpmSearchAPIResult | NpmSearchError> {
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

  const sortedRaw = [...raw].sort((a, b) => {
    const scoreA = (a as NpmCliSearchItem)?.score?.final ?? 0;
    const scoreB = (b as NpmCliSearchItem)?.score?.final ?? 0;
    return scoreB - scoreA;
  });
  const pageItems = sortedRaw.slice(from, from + limit) as unknown[];
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

export async function searchNpmPackageViaRegistrySearch(
  keywords: string,
  limit: number,
  fetchMetadata: boolean,
  from: number = 0
): Promise<NpmSearchAPIResult | NpmSearchError> {
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

    const sortedObjects = [...validation.data.objects].sort((a, b) => {
      const scoreA =
        (a.score as { final?: number } | null | undefined)?.final ?? 0;
      const scoreB =
        (b.score as { final?: number } | null | undefined)?.final ?? 0;
      return scoreB - scoreA;
    });

    const searchResults = (
      sortedObjects
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

export async function searchNpmPackageViaWebSearch(
  packageName: string,
  limit: number
): Promise<NpmSearchAPIResult | NpmSearchError> {
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

export async function searchNpmPackageViaSearch(
  keywords: string,
  limit: number,
  fetchMetadata: boolean,
  from: number = 0
): Promise<NpmSearchAPIResult | NpmSearchError> {
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

export async function enrichTopSearchResult(
  result: NpmSearchAPIResult
): Promise<NpmSearchAPIResult> {
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
