import { describe, it, expect, vi, beforeAll } from 'vitest';
import { executeBulkOperation } from '../../../octocode-tools-core/src/utils/response/bulk.js';
import { TOOL_NAMES } from '../../../octocode-tools-core/src/tools/toolMetadata/proxies.js';
import { getTextContent } from './testHelpers.js';

beforeAll(async () => {});

describe('Bulk Response Envelope', () => {
  const toolName = TOOL_NAMES.GITHUB_SEARCH_CODE;

  it('should return only results without a bulk instructions field', async () => {
    const queries = [{ id: 'q1' }];
    const processor = vi.fn().mockResolvedValue({
      data: { test: true },
    });

    const result = await executeBulkOperation(queries, processor, { toolName });
    const text = getTextContent(result.content);

    expect(text).toContain('results:');
    expect(text).toContain('id: q1');
    expect(text).not.toContain('instructions:');
    expect(text).not.toContain('original query');
  });

  it('should keep ids stable across mixed statuses', async () => {
    const queries = [
      { id: 'q1', type: 'hasResults' },
      { id: 'q2', type: 'empty' },
      { id: 'q3', type: 'error' },
    ];
    const processor = vi
      .fn()
      .mockImplementation(async (query: { type: string }) => {
        if (query.type === 'error') {
          return { status: 'error' as const, error: 'fail' };
        }
        return {
          status: query.type as 'hasResults' | 'empty',
          data: {},
        };
      });

    const result = await executeBulkOperation(queries, processor, { toolName });
    const text = getTextContent(result.content);

    expect(text).toContain('id: q1');
    expect(text).toContain('id: q2');
    expect(text).toContain('id: q3');
    expect(text).not.toContain('instructions:');
  });

  it('should include hints inside each result for present statuses with custom hints', async () => {
    const q1 = [{ id: '1' }];
    const p1 = vi.fn().mockResolvedValue({
      data: {},
      hints: ['Custom success hint'],
    });
    const r1 = await executeBulkOperation(q1, p1, { toolName });
    const t1 = getTextContent(r1.content);

    expect(t1).toContain('Custom success hint');
    expect(t1).not.toContain('hasResultsStatusHints');
    expect(t1).not.toContain('emptyStatusHints');
    expect(t1).not.toContain('errorStatusHints');

    const q2 = [{ id: '2' }];
    const p2 = vi.fn().mockResolvedValue({
      status: 'empty',
      data: {},
      hints: ['Custom empty hint'],
    });
    const r2 = await executeBulkOperation(q2, p2, { toolName });
    const t2 = getTextContent(r2.content);

    expect(t2).toContain('Custom empty hint');
    expect(t2).not.toContain('hasResultsStatusHints');
    expect(t2).not.toContain('emptyStatusHints');
    expect(t2).not.toContain('errorStatusHints');

    const q3 = [{ id: '3' }];
    const p3 = vi.fn().mockResolvedValue({
      status: 'error',
      error: 'e',
      hints: ['Custom error hint'],
    });
    const r3 = await executeBulkOperation(q3, p3, { toolName });
    const t3 = getTextContent(r3.content);

    expect(t3).toContain('Custom error hint');
    expect(t3).not.toContain('hasResultsStatusHints');
    expect(t3).not.toContain('emptyStatusHints');
    expect(t3).not.toContain('errorStatusHints');
  });
});
