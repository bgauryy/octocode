import type { AuthInfo } from '@modelcontextprotocol/server';
import {
  OctokitWithThrottling,
  resolveCacheAuthFingerprint,
} from './client.js';
import { withDataCache } from '../utils/http/cache/dataCache.js';
import { generateCacheKey } from '../utils/http/cache/key.js';

/**
 * Resolve an owner/repo to its canonical name, following GitHub repository
 * renames/redirects (the Search API does NOT follow renames, so a search
 * scoped to a stale name silently returns zero results — a false absence).
 * `repos.get` transparently redirects to the new location, so its `full_name`
 * is authoritative. Returns the original pair on any failure (network, 404,
 * private) so callers degrade gracefully rather than blocking the search.
 */
export async function resolveCanonicalOwnerRepo(
  octokit: InstanceType<typeof OctokitWithThrottling>,
  owner: string,
  repo: string,
  authInfo?: AuthInfo
): Promise<{ owner: string; repo: string; renamed: boolean }> {
  try {
    const auth = await resolveCacheAuthFingerprint(authInfo);
    const response = await withDataCache(
      generateCacheKey('gh-canonical-repo', { owner, repo, auth }),
      () => octokit.rest.repos.get({ owner, repo }),
      {
        cacheRole: 'helper',
        ttl: 300,
        shouldCache: value => {
          const [canonicalOwner, canonicalRepo] =
            value.data.full_name?.split('/') ?? [];
          return Boolean(canonicalOwner && canonicalRepo);
        },
      }
    );
    const [canonicalOwner, canonicalRepo] =
      response.data.full_name?.split('/') ?? [];
    if (!canonicalOwner || !canonicalRepo) {
      return { owner, repo, renamed: false };
    }
    const renamed = canonicalOwner !== owner || canonicalRepo !== repo;
    return { owner: canonicalOwner, repo: canonicalRepo, renamed };
  } catch {
    return { owner, repo, renamed: false };
  }
}
