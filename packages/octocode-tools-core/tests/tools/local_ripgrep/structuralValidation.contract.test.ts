import { beforeEach, describe, expect, it, vi } from 'vitest';

const searchContentRipgrep = vi.hoisted(() => vi.fn());
vi.mock('node:fs/promises', async importOriginal => ({
  ...(await importOriginal<typeof import('node:fs/promises')>()),
  access: vi.fn().mockResolvedValue(undefined),
}));
vi.mock('../../../src/tools/local_ripgrep/searchContentRipgrep.js', () => ({
  searchContentRipgrep,
}));

import { executeLocalSearch } from '../../../src/tools/local_search/execution.js';
import type { LocalSearchQuery } from '../../../src/tools/local_search/scheme.js';
import { LocalRipgrepQuerySchema } from '../../../src/tools/local_ripgrep/scheme.js';

const emptyQueries = (['pattern', 'rule'] as const).flatMap(field =>
  ['', '   ', '\n\t'].map(value => ({ field, value }))
);

describe('structural frontend validation contract', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    searchContentRipgrep.mockResolvedValue({
      searchEngine: 'structural',
      stats: { totalStructuralMatches: 0 },
    });
  });

  it.each(emptyQueries)(
    'rejects blank $field at the execution schema',
    ({ field, value }) => {
      expect(
        LocalRipgrepQuerySchema.safeParse({
          path: '/repo',
          mode: 'structural',
          [field]: value,
        }).success
      ).toBe(false);
    }
  );

  it.each(emptyQueries)(
    'keeps a typed row and CLI/MCP metadata for blank $field',
    async ({ field, value }) => {
      const result = await executeLocalSearch({
        queries: [
          {
            path: '/repo',
            operation: 'structural',
            [field]: value,
          } as LocalSearchQuery,
        ],
      });
      expect(result).toMatchObject({
        structuredContent: {
          results: [
            {
              status: 'error',
              meta: {
                evidence: { kind: 'structural', confidence: 'low' },
                diagnostics: { codes: ['structural.query.invalid'] },
              },
              data: { errorCode: 'structural.query.invalid' },
            },
          ],
        },
      });
      expect(JSON.stringify(result.content)).toContain(
        'structural.query.invalid'
      );
      expect(JSON.stringify(result)).not.toContain('toolExecutionFailed');
      expect(searchContentRipgrep).not.toHaveBeenCalled();
    }
  );

  it('preserves surrounding whitespace in valid patterns', async () => {
    const pattern = '  target($X)  ';
    await executeLocalSearch({
      queries: [
        { path: '/repo', operation: 'structural', pattern } as LocalSearchQuery,
      ],
    });
    expect(searchContentRipgrep).toHaveBeenCalledWith(
      expect.objectContaining({ pattern })
    );
  });
});
