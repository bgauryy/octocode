import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import {
  clearAllCache,
  withDataCache,
  withDataCacheConditional,
} from '../../../src/utils/http/cache.js';
import { executeBulkOperation } from '../../../src/utils/response/bulk.js';
import { cache } from '../../../src/utils/http/cache/store.js';

type CacheRunner = (
  key: string,
  operation: () => Promise<string>
) => Promise<string>;

const runners: Array<[string, CacheRunner]> = [
  ['data cache', (key, operation) => withDataCache(key, operation)],
  [
    'conditional cache',
    (key, operation) =>
      withDataCacheConditional(key, async () => ({
        value: await operation(),
      })),
  ],
];

function responseRows(
  result: Awaited<ReturnType<typeof executeBulkOperation>>
) {
  return (
    result.structuredContent as { results: Array<Record<string, unknown>> }
  ).results;
}

describe.each(runners)('%s response marker', (_name, runCached) => {
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

  it('emits cache: 1 only when the primary response came from cache', async () => {
    const key = `v1-gh-api-code:response-marker-${_name}`;
    let upstreamCalls = 0;
    const execute = () =>
      executeBulkOperation(
        [{ id: 'query' }],
        async () => ({
          value: await runCached(key, async () => {
            upstreamCalls++;
            return 'payload';
          }),
        }),
        { toolName: 'github.code' }
      );

    const fresh = await execute();
    expect(responseRows(fresh)[0]).not.toHaveProperty('cache');
    expect(fresh.content[0]).not.toMatchObject({
      type: 'text',
      text: expect.stringContaining('cache: 1'),
    });

    const cached = await execute();
    expect(responseRows(cached)[0]).toMatchObject({ cache: 1 });
    expect(cached.content[0]).toMatchObject({
      type: 'text',
      text: expect.stringContaining('cache: 1'),
    });
    expect(upstreamCalls).toBe(1);
  });

  it('preserves the marker when a tool uses a custom finalizer', async () => {
    const key = `v1-gh-api-code:finalized-marker-${_name}`;
    const execute = () =>
      executeBulkOperation(
        [{ id: 'query' }],
        async () => ({
          value: await runCached(key, async () => 'payload'),
        }),
        {
          toolName: 'github.code',
          finalize: ({ results }) => ({
            structuredContent: {
              results: results.map(row => ({
                index: row.index,
                value: row.data.value,
              })),
            },
            text: 'custom response',
          }),
        }
      );

    expect(responseRows(await execute())[0]).not.toHaveProperty('cache');
    expect(responseRows(await execute())[0]).toMatchObject({ cache: 1 });
  });
});

describe('nested cache marker isolation', () => {
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

  it('does not mark a fresh response when only a nested helper cache hits', async () => {
    await withDataCache('v1-gh-api-ref-pointer:nested', async () => 'pointer');

    const result = await executeBulkOperation(
      [{ id: 'query' }],
      async () => ({
        value: await withDataCache('v1-gh-api-code:outer', async () => {
          await withDataCache('v1-gh-api-ref-pointer:nested', async () =>
            Promise.reject(new Error('nested cache missed'))
          );
          return 'fresh payload';
        }),
      }),
      { toolName: 'github.code' }
    );

    expect(responseRows(result)[0]).not.toHaveProperty('cache');
  });

  it.each([
    [
      'data cache',
      (key: string, operation: () => Promise<string>) =>
        withDataCache(key, operation, { cacheRole: 'helper' }),
    ],
    [
      'conditional cache',
      (key: string, operation: () => Promise<string>) =>
        withDataCacheConditional(
          key,
          async () => ({ value: await operation() }),
          { cacheRole: 'helper' }
        ),
    ],
  ])(
    'does not mark a fresh response when only a sequential %s helper hits',
    async (_name, runHelper) => {
      const helperKey = `v1-gh-api-ref-pointer:sequential-${_name}`;
      await runHelper(helperKey, async () => 'cached helper');

      const result = await executeBulkOperation(
        [{ id: 'query' }],
        async () => {
          const value = 'fresh payload';
          await runHelper(helperKey, async () =>
            Promise.reject(new Error('helper cache missed'))
          );
          return { value };
        },
        { toolName: 'ghGetFileContent' }
      );

      expect(responseRows(result)[0]).not.toHaveProperty('cache');
    }
  );
});

describe('cache marker edge paths', () => {
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

  it('marks a conditional 304 recovery as a cache hit', async () => {
    const key = 'v1-gh-api-file-content:response-marker-304';
    let upstreamCalls = 0;
    const execute = () =>
      executeBulkOperation(
        [{ id: 'query' }],
        async () => ({
          value: await withDataCacheConditional(
            key,
            async ({ ifNoneMatch }) => {
              upstreamCalls++;
              if (ifNoneMatch) {
                return {
                  value: 'unused',
                  etag: ifNoneMatch,
                  notModified: true,
                };
              }
              return { value: 'payload', etag: '"v1"' };
            }
          ),
        }),
        { toolName: 'ghGetFileContent' }
      );

    expect(responseRows(await execute())[0]).not.toHaveProperty('cache');
    cache.del(key);
    expect(responseRows(await execute())[0]).toMatchObject({ cache: 1 });
    expect(upstreamCalls).toBe(2);
  });

  it('isolates cache markers between concurrent bulk queries', async () => {
    const hitKey = 'v1-gh-api-code:parallel-hit';
    await withDataCache(hitKey, async () => 'cached payload');

    const result = await executeBulkOperation(
      [{ key: hitKey }, { key: 'v1-gh-api-code:parallel-miss' }],
      async query => ({
        value: await withDataCache(query.key, async () => 'fresh payload'),
      }),
      { toolName: 'github.code', concurrency: 2 }
    );

    expect(responseRows(result)[0]).toMatchObject({ cache: 1 });
    expect(responseRows(result)[1]).not.toHaveProperty('cache');
  });
});
