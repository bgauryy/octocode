import { executeNpmCommand } from '../../exec/npm.js';
import { generateCacheKey } from '../../http/cache/key.js';
import { withDataCache } from '../../http/cache/dataCache.js';
import type {
  DeprecationInfo,
  NpmSearchAPIResult,
  NpmSearchError,
} from '../types.js';
import { NpmDeprecationOutputSchema } from '../schemas.js';
import { resolveNpmRegistryContext } from './npmRegistry.js';
import { fetchNpmPackage } from './npmDetailsFetchers.js';
import { searchNpmPackageViaRegistrySearch } from './npmSearchStrategies.js';

export function isExactPackageName(query: string): boolean {
  if (query.startsWith('@') && query.includes('/')) {
    return true;
  }
  if (query.includes(' ')) {
    return false;
  }
  return /^[a-z0-9][a-z0-9._-]*$/i.test(query);
}

export async function searchNpmPackage(
  packageName: string,
  limit: number,
  fetchMetadata: boolean,
  from: number = 0,
  mode: 'exact' | 'keywords' = 'exact',
  registryOverride?: string
): Promise<NpmSearchAPIResult | NpmSearchError> {
  const context = await resolveNpmRegistryContext(
    mode === 'exact' ? packageName : undefined,
    registryOverride
  );
  const cacheKey = generateCacheKey('npm-search', {
    registry: context.registry,
    identity: context.cacheIdentity,
    mode,
    name: packageName,
    limit,
    metadata: fetchMetadata,
    from,
  });

  return withDataCache(
    cacheKey,
    async () => {
      // Keep the selector and provider stable across every continuation. A
      // keyword that happens to be a valid package name is still discovery.
      const result =
        mode === 'exact'
          ? await fetchNpmPackage(packageName, fetchMetadata, context)
          : await searchNpmPackageViaRegistrySearch(
              packageName,
              limit,
              fetchMetadata,
              from,
              context
            );
      return 'error' in result
        ? result
        : { ...result, registry: context.registry };
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

    if (!output || output === 'undefined') {
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
