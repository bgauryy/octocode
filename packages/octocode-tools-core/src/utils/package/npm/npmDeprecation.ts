import { executeNpmCommand } from '../../exec/npm.js';
import { generateCacheKey, withDataCache } from '../../http/cache.js';
import { isCircuitOpen } from '../../http/circuitBreaker.js';
import type {
  DeprecationInfo,
  NpmSearchAPIResult,
  NpmSearchError,
} from '../types.js';
import { NpmDeprecationOutputSchema } from '../schemas.js';
import { DEFAULT_NPM_REGISTRY } from './npmRegistry.js';
import {
  enrichPackageDetails,
  fetchNpmPackageByView,
  fetchPackageDetailsFromCdn,
  isNetworkFetchError,
} from './npmDetailsFetchers.js';
import {
  enrichTopSearchResult,
  searchNpmPackageViaSearch,
  searchNpmPackageViaWebSearch,
} from './npmSearchStrategies.js';

export function isExactPackageName(query: string): boolean {
  if (query.startsWith('@') && query.includes('/')) {
    return true;
  }
  if (query.includes(' ')) {
    return false;
  }
  return /^[a-z0-9][a-z0-9._-]*$/i.test(query);
}

export function _packageNameToSearchKeywords(packageName: string): string {
  return packageName
    .replace(/^@/, '')
    .replace(/[/_-]/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

export async function searchNpmPackage(
  packageName: string,
  limit: number,
  fetchMetadata: boolean,
  from: number = 0
): Promise<NpmSearchAPIResult | NpmSearchError> {
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
        if (from === 0 && limit === 1 && isExactPackageName(packageName)) {
          const cdnResult = await fetchPackageDetailsFromCdn(
            packageName,
            fetchMetadata
          );
          if (cdnResult.pkg) {
            const enriched = await enrichPackageDetails(
              packageName,
              cdnResult.pkg,
              cdnResult.rawResponseChars
            );
            return {
              packages: enriched.pkg ? [enriched.pkg] : [],
              totalFound: enriched.pkg ? 1 : 0,
              rawResponseChars: enriched.rawResponseChars,
            };
          }
        }

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
          hints: ['Use `ghSearchRepos` to find the source repo directly.'],
        } as NpmSearchError;
      }

      if (from === 0 && isExactPackageName(packageName)) {
        const exactResult = await fetchNpmPackageByView(
          packageName,
          fetchMetadata,
          limit === 1
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
            'Use `ghSearchRepos` to find the source repo directly by package name or domain terms.',
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
