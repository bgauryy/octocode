import { beforeEach, describe, expect, it, vi } from 'vitest';

const searchContentStructural = vi.hoisted(() => vi.fn());
vi.mock('node:fs/promises', async importOriginal => ({
  ...(await importOriginal<typeof import('node:fs/promises')>()),
  access: vi.fn().mockResolvedValue(undefined),
}));
vi.mock('../../../src/tools/local_ripgrep/structuralSearch.js', () => ({
  searchContentStructural,
}));

import { executeAstSearch } from '../../../src/tools/ast_search/execution.js';
import type { AstSearchQuery } from '@octocodeai/octocode-core/schema';
import { LocalRipgrepQuerySchema } from '@octocodeai/octocode-core/schema';

const emptyQueries = (['pattern', 'rule'] as const).flatMap(field =>
  ['', '   ', '\n\t'].map(value => ({ field, value }))
);

describe('structural frontend validation contract', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    searchContentStructural.mockResolvedValue({
      searchEngine: 'structural',
      stats: { totalStructuralMatches: 0 },
    });
  });

  it.each(emptyQueries)(
    'rejects blank $field at the execution schema',
    ({ field, value }) => {
      expect(
        LocalRipgrepQuerySchema.safeParse({
          reasoning: 'Reject blank structural patterns at the execution schema.',
          path: process.cwd(), langType: 'ts',
          mode: 'structural',
          [field]: value,
        }).success
      ).toBe(false);
    }
  );

  it.each(emptyQueries)(
    'keeps a typed row and CLI/MCP metadata for blank $field',
    async ({ field, value }) => {
      const result = await executeAstSearch({
        queries: [
          {
            reasoning: 'Return a typed structural validation error.',
            debug: true,
            path: process.cwd(), langType: 'ts',
            operation: 'match',
            [field]: value,
          } as AstSearchQuery,
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
      expect(searchContentStructural).not.toHaveBeenCalled();
    }
  );

  it('preserves surrounding whitespace in valid patterns', async () => {
    const pattern = '  target($X)  ';
    await executeAstSearch({
      queries: [
        { path: process.cwd(), langType: 'ts', operation: 'match', pattern } as AstSearchQuery,
      ],
    });
    expect(searchContentStructural).toHaveBeenCalledWith(
      expect.objectContaining({ pattern })
    );
  });
});
