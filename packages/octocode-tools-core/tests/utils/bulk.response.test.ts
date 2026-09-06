import { describe, expect, it } from 'vitest';

import { executeBulkOperation } from '../../src/utils/response/bulk/response.js';
import { formatFinalizedResponse } from '../../src/utils/response/groupedFinalizer.js';

describe('executeBulkOperation batch correlation', () => {
  it('normalizes custom-finalizer cache markers to the strict cache: 1 contract', async () => {
    const finalize = ({ results }: { results: Array<{ index: number }> }) => ({
      structuredContent: {
        results: results.map(row => ({ index: row.index, cache: true })),
      },
    });

    const fresh = await executeBulkOperation(
      [{ value: 'fresh' }],
      async query => query,
      { toolName: 'testTool', finalize }
    );
    expect(fresh.structuredContent).toMatchObject({ results: [{ index: 0 }] });
    expect(
      (fresh.structuredContent as { results: unknown[] }).results[0]
    ).not.toHaveProperty('cache');

    const cached = await executeBulkOperation(
      [{ value: 'cached' }],
      async query => ({ ...query, cache: 1 as const }),
      { toolName: 'testTool', finalize }
    );
    expect(cached.structuredContent).toMatchObject({
      results: [{ index: 0, cache: 1 }],
    });
  });

  it('keeps finalized and ordinary response-envelope hoisting in parity', async () => {
    const queries = [{ value: 'one' }, { value: 'two' }];
    const processor = async (query: { value: string }) => ({
      owner: 'octocode',
      path: `/workspace/src/${query.value}.ts`,
    });

    const ordinary = await executeBulkOperation(queries, processor, {
      toolName: 'testTool',
    });
    const finalized = await executeBulkOperation(queries, processor, {
      toolName: 'testTool',
      finalize: ({ results }) =>
        formatFinalizedResponse({ results }, [
          'results',
          'index',
          'status',
          'meta',
          'data',
          'owner',
          'path',
        ]),
    });

    expect(finalized.structuredContent).toEqual(ordinary.structuredContent);
  });

  it('reconciles continuation diagnostics after a finalizer adds next-page data', async () => {
    const result = await executeBulkOperation(
      [{ page: 1 }],
      async () => ({ pagination: { hasMore: true } }),
      {
        toolName: 'ghSearch',
        finalize: ({ results }) =>
          formatFinalizedResponse(
            {
              results: results.map(row => ({
                ...row,
                data: {
                  ...row.data,
                  next: {
                    nextPage: {
                      tool: 'ghSearch',
                      query: { operation: 'code', page: 2 },
                    },
                  },
                },
              })),
            },
            ['results', 'index', 'meta', 'data', 'pagination', 'next']
          ),
      }
    );

    expect(result.structuredContent).toMatchObject({
      results: [
        {
          meta: { diagnostics: { partial: true } },
          data: { next: { nextPage: { tool: 'ghSearch' } } },
        },
      ],
    });
    const [row] = (
      result.structuredContent as {
        results: Array<{ meta: { diagnostics?: { codes?: string[] } } }>;
      }
    ).results;
    expect(row?.meta.diagnostics?.codes ?? []).not.toContain(
      'continuationMissing'
    );
    const text =
      result.content[0]?.type === 'text' ? result.content[0].text : '';
    expect(text).not.toContain('continuationMissing');
    expect(text).toContain('nextPage');
  });

  it('includes finalized shared partial state in each row diagnostic', async () => {
    const result = await executeBulkOperation(
      [{ path: 'file.ts' }],
      async () => ({ value: 'slice' }),
      {
        toolName: 'ghGetFileContent',
        finalize: ({ results }) => ({
          structuredContent: {
            shared: { isPartial: true },
            results: results.map(row => ({
              ...row,
              data: {
                ...row.data,
                next: {
                  continueLines: {
                    tool: 'ghGetFileContent',
                    query: { path: 'file.ts', startLine: 11, endLine: 20 },
                  },
                },
              },
            })),
          },
        }),
      }
    );

    expect(result.structuredContent).toMatchObject({
      results: [{ meta: { diagnostics: { partial: true } } }],
    });
  });

  it('labels whole-response pagination as text-channel-only', async () => {
    const result = await executeBulkOperation(
      [{ value: 'x'.repeat(200) }],
      async query => query,
      { toolName: 'testTool' },
      { responseCharLength: 40 }
    );

    expect(result.structuredContent).toMatchObject({
      responsePagination: {
        scope: 'content.text',
        hasMore: true,
        charOffset: 0,
        next: {
          tool: 'testTool',
          query: {
            queries: [{ value: 'x'.repeat(200) }],
            responseCharLength: 40,
            responseCharOffset: expect.any(Number),
          },
        },
      },
    });
    expect(result.structuredContent).toMatchObject({
      results: [{ data: { value: 'x'.repeat(200) } }],
    });
  });

  it('returns one ordered index row per query and isolates query failures', async () => {
    const result = await executeBulkOperation(
      [
        { id: 'caller-slow', label: 'slow' },
        { id: 'caller-error', label: 'error' },
        { label: 'fast' },
      ],
      async query => {
        if (query.label === 'error') throw new Error('isolated failure');
        if (query.label === 'slow') {
          await new Promise(resolve => setTimeout(resolve, 10));
        }
        return { value: query.label };
      },
      { toolName: 'testTool', concurrency: 3 }
    );

    const structured = result.structuredContent as {
      results: Array<{
        index: number;
        status?: string;
        data: Record<string, unknown>;
      }>;
    };

    expect(structured.results).toHaveLength(3);
    expect(structured.results.map(row => row.index)).toEqual([0, 1, 2]);
    expect(structured.results[0]?.data).toEqual({ value: 'slow' });
    expect(structured.results[1]).toMatchObject({
      index: 1,
      status: 'error',
      data: { error: 'isolated failure' },
    });
    expect(structured.results[2]?.data).toEqual({ value: 'fast' });
    expect(JSON.stringify(structured)).not.toContain('caller-slow');
    expect(JSON.stringify(structured)).not.toContain('caller-error');
    expect(result.isError).toBe(false);
  });
});
