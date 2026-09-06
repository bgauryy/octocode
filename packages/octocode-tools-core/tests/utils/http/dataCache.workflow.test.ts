import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { clearAllCache } from '../../../src/utils/http/cache/management.js';
import { withDataCache } from '../../../src/utils/http/cache/dataCache.js';
import { withDataCacheConditional } from '../../../src/utils/http/cache/conditional.js';
import {
  pendingRequests,
  PENDING_REQUEST_MAX_AGE_MS,
} from '../../../src/utils/http/cache/store.js';

type CacheRunner = (
  key: string,
  operation: () => Promise<string>
) => Promise<string>;

const runners: Array<[string, CacheRunner]> = [
  ['data cache', (key, operation) => withDataCache(key, operation)],
  [
    'conditional data cache',
    (key, operation) =>
      withDataCacheConditional(key, async () => ({
        value: await operation(),
      })),
  ],
];

describe.each(runners)('%s stale-request workflow', (_name, runCached) => {
  let previousDiskCache: string | undefined;

  beforeEach(() => {
    previousDiskCache = process.env.OCTOCODE_DISK_CACHE;
    process.env.OCTOCODE_DISK_CACHE = 'false';
    clearAllCache();
  });

  afterEach(() => {
    clearAllCache();
    if (previousDiskCache === undefined) delete process.env.OCTOCODE_DISK_CACHE;
    else process.env.OCTOCODE_DISK_CACHE = previousDiskCache;
  });

  it('keeps the replacement request tracked when an evicted request settles', async () => {
    const key = 'v2-gh-api-prs:stale-replacement';
    let rejectOld!: (error: Error) => void;
    let resolveReplacement!: (value: string) => void;
    let upstreamCalls = 0;

    const oldRequest = runCached(key, async () => {
      upstreamCalls++;
      await new Promise<never>((_, reject) => {
        rejectOld = reject;
      });
      return 'old';
    }).catch(() => 'old-failed');

    await new Promise(resolve => setImmediate(resolve));
    const oldPending = pendingRequests.get(key);
    expect(oldPending).toBeDefined();
    oldPending!.startedAt = Date.now() - PENDING_REQUEST_MAX_AGE_MS - 1;

    const replacement = runCached(key, async () => {
      upstreamCalls++;
      return await new Promise<string>(resolve => {
        resolveReplacement = resolve;
      });
    });
    await new Promise(resolve => setImmediate(resolve));

    rejectOld(new Error('stale request failed'));
    await oldRequest;

    expect(pendingRequests.has(key)).toBe(true);

    const coalesced = runCached(key, async () => {
      upstreamCalls++;
      return 'duplicate';
    });
    await new Promise(resolve => setImmediate(resolve));
    expect(upstreamCalls).toBe(2);

    resolveReplacement('fresh');
    await expect(Promise.all([replacement, coalesced])).resolves.toEqual([
      'fresh',
      'fresh',
    ]);
  });
});
