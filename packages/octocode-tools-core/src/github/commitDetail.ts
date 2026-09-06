import type { AuthInfo } from '@modelcontextprotocol/server';
import { getOctokit, resolveCacheAuthFingerprint } from './client.js';
import { generateCacheKey } from '../utils/http/cache/key.js';
import { withDataCache } from '../utils/http/cache/dataCache.js';
import { countSerializedChars } from '../utils/response/charSavings.js';

export const COMMIT_FILE_LIMIT = 3000;
const FILES_PER_PAGE = 100;
type CommitData = Awaited<
  ReturnType<
    Awaited<ReturnType<typeof getOctokit>>['rest']['repos']['getCommit']
  >
>['data'];

/** Fetch one auth-scoped provider batch; callers expose independent file and batch continuations. */
export async function fetchCommitDetail(
  params: { owner: string; repo: string; ref: string; fileBatch?: number },
  authInfo?: AuthInfo
): Promise<{
  data: CommitData;
  collectionState: { page: number; hasMore: boolean };
  terminalLimit: boolean;
  rawResponseChars: number;
}> {
  const auth = await resolveCacheAuthFingerprint(authInfo);
  const { fileBatch = 1, ...identity } = params;
  const key = generateCacheKey('gh-commit-detail-page', {
    ...identity,
    fileBatch,
    auth,
  });
  return withDataCache(key, async () => {
    const octokit = await getOctokit(authInfo);
    const first = await octokit.rest.repos.getCommit({
      ...identity,
      per_page: FILES_PER_PAGE,
      page: fileBatch,
    });
    const files = first.data.files ?? [];
    const hasMore = /;\s*rel="next"/.test(first.headers?.link ?? '');
    const terminalLimit =
      fileBatch * FILES_PER_PAGE >= COMMIT_FILE_LIMIT &&
      (files.length >= FILES_PER_PAGE || hasMore);
    return {
      data: first.data,
      collectionState: { page: fileBatch, hasMore: hasMore && !terminalLimit },
      terminalLimit,
      rawResponseChars: countSerializedChars(first.data),
    };
  });
}
