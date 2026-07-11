import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  clearAllCache,
  generateCacheKey,
  getCacheStats,
  withDataCacheConditional,
} from '../../src/utils/http/cache.js';
import { extractEtag } from '../../src/github/responseHeaders.js';
import { RequestError } from 'octokit';
import { fetchRawGitHubFileContent } from '../../src/github/fileContentRaw.js';

vi.mock('../../src/github/client.js', () => ({
  getOctokit: vi.fn(),
  resolveDefaultBranch: vi.fn(async () => 'main'),
  OctokitWithThrottling: class {},
}));

import { getOctokit } from '../../src/github/client.js';

describe('extractEtag', () => {
  it('reads etag / ETag / weak variants', () => {
    expect(extractEtag({ etag: '"abc"' })).toBe('"abc"');
    expect(extractEtag({ ETag: 'W/"weak"' })).toBe('W/"weak"');
    expect(extractEtag({})).toBeUndefined();
  });
});

describe('withDataCacheConditional (Phase C)', () => {
  afterEach(() => {
    clearAllCache();
  });

  it('stores soft etag; after primary TTL miss a 304 returns the prior body', async () => {
    const key = generateCacheKey('gh-api-file-content', {
      owner: 'o',
      repo: 'r',
      path: 'a.ts',
      etagTest: '304-path',
    });
    let calls = 0;

    const first = await withDataCacheConditional(
      key,
      async ({ ifNoneMatch }) => {
        calls++;
        expect(ifNoneMatch).toBeUndefined();
        return {
          value: { data: { rawContent: 'hello' }, status: 200 },
          etag: '"v1"',
        };
      },
      { ttl: 1 }
    );
    expect(first).toEqual({ data: { rawContent: 'hello' }, status: 200 });

    // Primary TTL hit — no operation call.
    const hit = await withDataCacheConditional(
      key,
      async () => {
        throw new Error('must not fetch while primary TTL is warm');
      },
      { ttl: 1 }
    );
    expect(hit).toEqual(first);
    expect(calls).toBe(1);

    await new Promise(resolve => setTimeout(resolve, 1100));

    const statsBefore = getCacheStats();
    const third = await withDataCacheConditional(
      key,
      async ({ ifNoneMatch }) => {
        calls++;
        expect(ifNoneMatch).toBe('"v1"');
        return {
          value: { data: { rawContent: '' }, status: 304 },
          etag: '"v1"',
          notModified: true,
        };
      },
      { ttl: 1 }
    );

    expect(third).toEqual({ data: { rawContent: 'hello' }, status: 200 });
    expect(calls).toBe(2);
    expect(getCacheStats().hits).toBeGreaterThan(statsBefore.hits);
  });

  it('forceRefresh bypasses soft etag (no If-None-Match)', async () => {
    const key = generateCacheKey('gh-api-file-content', {
      owner: 'o',
      repo: 'r',
      path: 'b.ts',
      etagTest: 'force',
    });

    await withDataCacheConditional(
      key,
      async () => ({
        value: { data: { rawContent: 'old' }, status: 200 },
        etag: '"old"',
      }),
      { ttl: 60 }
    );

    let sawIfNone: string | undefined = 'sentinel';
    const fresh = await withDataCacheConditional(
      key,
      async ({ ifNoneMatch }) => {
        sawIfNone = ifNoneMatch;
        return {
          value: { data: { rawContent: 'new' }, status: 200 },
          etag: '"new"',
        };
      },
      { ttl: 60, forceRefresh: true }
    );

    expect(sawIfNone).toBeUndefined();
    expect(fresh).toEqual({ data: { rawContent: 'new' }, status: 200 });
  });

  it('does not soft-cache errors', async () => {
    const key = generateCacheKey('gh-api-file-content', {
      owner: 'o',
      repo: 'r',
      path: 'err.ts',
      etagTest: 'err',
    });

    const err = await withDataCacheConditional(
      key,
      async () => ({
        value: { error: 'boom', status: 500, type: 'unknown' as const },
        etag: '"should-not-stick"',
      }),
      {
        ttl: 60,
        shouldCache: value => !('error' in value),
      }
    );
    expect('error' in err).toBe(true);

    let calls = 0;
    await withDataCacheConditional(
      key,
      async ({ ifNoneMatch }) => {
        calls++;
        expect(ifNoneMatch).toBeUndefined();
        return {
          value: { data: { rawContent: 'ok' }, status: 200 },
          etag: '"ok"',
        };
      },
      {
        ttl: 60,
        shouldCache: value => !('error' in value),
      }
    );
    expect(calls).toBe(1);
  });
});

describe('fetchRawGitHubFileContent 304', () => {
  afterEach(() => {
    vi.mocked(getOctokit).mockReset();
    clearAllCache();
  });

  it('returns notModified when Octokit raises 304', async () => {
    const getContent = vi.fn(async () => {
      throw new RequestError('Not Modified', 304, {
        request: {
          method: 'GET',
          url: 'https://api.github.com',
          headers: {},
        },
        response: {
          status: 304,
          url: 'https://api.github.com',
          headers: {},
          data: {},
        },
      });
    });
    vi.mocked(getOctokit).mockResolvedValue({
      rest: { repos: { getContent } },
    } as never);

    const result = await fetchRawGitHubFileContent(
      { owner: 'o', repo: 'r', path: 'f.ts', branch: 'main' },
      undefined,
      { ifNoneMatch: '"etag-1"' }
    );

    expect(result.notModified).toBe(true);
    expect(result.status).toBe(304);
    expect(result.etag).toBe('"etag-1"');
    expect(getContent).toHaveBeenCalledWith(
      expect.objectContaining({
        headers: { 'If-None-Match': '"etag-1"' },
      })
    );
  });
});
