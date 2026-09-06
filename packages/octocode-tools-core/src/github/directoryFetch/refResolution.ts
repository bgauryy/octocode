import type { AuthInfo } from '@modelcontextprotocol/server';
import { getOctokit, resolveCacheAuthFingerprint } from '../client.js';
import { generateCacheKey } from '../../utils/http/cache/key.js';
import { withDataCache } from '../../utils/http/cache/dataCache.js';

const REF_POINTER_TTL_SECONDS = 60;

export interface MaterializationRef {
  commitSha: string;
  resolvedRef: string;
}

async function fetchCommitSha(
  owner: string,
  repo: string,
  ref: string,
  authInfo?: AuthInfo
): Promise<string> {
  const octokit = await getOctokit(authInfo);
  const result = await octokit.rest.repos.getCommit({ owner, repo, ref });
  const sha = result.data.sha;
  if (typeof sha !== 'string' || !/^[0-9a-f]{40}$/i.test(sha)) {
    throw new Error(`GitHub returned an invalid commit SHA for ref "${ref}".`);
  }
  return sha.toLowerCase();
}

async function resolveRefUncached(
  owner: string,
  repo: string,
  ref: string,
  authInfo?: AuthInfo
): Promise<MaterializationRef> {
  return {
    commitSha: await fetchCommitSha(owner, repo, ref, authInfo),
    resolvedRef: ref,
  };
}

export async function resolveMaterializationRef(
  owner: string,
  repo: string,
  ref: string,
  authInfo?: AuthInfo,
  forceRefresh = false
): Promise<MaterializationRef> {
  const auth = await resolveCacheAuthFingerprint(authInfo);
  const cacheKey = generateCacheKey('gh-api-ref-resolution', {
    owner,
    repo,
    ref,
    auth,
  });
  return withDataCache(
    cacheKey,
    () => resolveRefUncached(owner, repo, ref, authInfo),
    {
      ttl: REF_POINTER_TTL_SECONDS,
      forceRefresh,
      cacheRole: 'helper',
    }
  );
}
