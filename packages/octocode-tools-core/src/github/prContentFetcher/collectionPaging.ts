import type { AuthInfo } from '@modelcontextprotocol/server';
import { resolveCacheAuthFingerprint } from '../client.js';
import { generateCacheKey } from '../../utils/http/cache/key.js';
import { withDataCache } from '../../utils/http/cache/dataCache.js';
import { countSerializedChars } from '../../utils/response/charSavings.js';

export type CollectionSurface =
  'changedFiles' | 'discussion' | 'inline' | 'reviews' | 'commits';
export type CollectionPages = Partial<Record<CollectionSurface, number>>;
export type CollectionState = { page: number; hasMore: boolean };
export type CollectionStates = Partial<
  Record<CollectionSurface, CollectionState>
>;
export type CollectionArray<T> = T[] & { collectionState?: CollectionState };

/** One cached provider page per requested surface. Zero marks an exhausted source. */
export async function fetchCollectionPage<T>(
  identity: {
    owner: string;
    repo: string;
    prNumber: number;
    surface: CollectionSurface;
  },
  page: number,
  fetchPage: (
    page: number
  ) => Promise<{ data: T[]; headers?: { link?: string } }>,
  authInfo?: AuthInfo
): Promise<{
  items: T[];
  rawResponseChars: number;
  collectionState: CollectionState;
}> {
  if (page === 0)
    return {
      items: [],
      rawResponseChars: 0,
      collectionState: { page: 0, hasMore: false },
    };
  const auth = await resolveCacheAuthFingerprint(authInfo);
  return withDataCache(
    generateCacheKey('gh-pr-collection-page', { ...identity, page, auth }),
    async () => {
      const response = await fetchPage(page);
      return {
        items: response.data,
        rawResponseChars: countSerializedChars(response.data),
        collectionState: {
          page,
          hasMore: /;\s*rel="next"/.test(response.headers?.link ?? ''),
        },
      };
    }
  );
}
