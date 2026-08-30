import { describe, expect, it } from 'vitest';

import { executeBulkOperation } from '../../src/utils/response/bulk.js';
import { formatFinalizedResponse } from '../../src/utils/response/groupedFinalizer.js';

describe('executeBulkOperation batch correlation', () => {
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
