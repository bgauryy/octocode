import { describe, expect, it } from 'vitest';
import { rewriteError } from '../../../src/tools/ast_rewrite/result.js';
import { executeBulkOperation } from '../../../src/utils/response/bulk/response.js';

async function response(data: Record<string, unknown>) {
  const result = await executeBulkOperation(
    [
      {
        reasoning: 'Exercise astRewrite result evidence and diagnostics.',
        debug: true,
      },
    ],
    async () => data,
    {
      toolName: 'astRewrite',
    }
  );
  return result.structuredContent as {
    results: Array<{
      status?: string;
      data: Record<string, unknown>;
      meta: { diagnostics?: { partial?: boolean; codes?: string[] } };
    }>;
  };
}

describe('rewrite error evidence contract', () => {
  it.each([
    ['ast.rewrite.expected_hash_invalid', { path: 'source.ts' }],
    [
      'ast.rewrite.transaction_failed',
      { rollback: { restored: false, errors: ['Concurrent edit retained'] } },
    ],
  ])('does not label %s as omitted evidence', async (code, details) => {
    const {
      results: [row],
    } = await response(rewriteError(code, 'Operation failed', { details }));
    expect(row?.status).toBe('error');
    expect(row?.data).toMatchObject({ errorCode: code, details });
    expect(row?.data).not.toHaveProperty('complete');
    expect(row?.data).not.toHaveProperty('isPartial');
    expect(row?.meta.diagnostics).toEqual({ codes: [code] });
  });

  it('retains explicit terminal diagnostics for bounded rewrite output', async () => {
    const {
      results: [row],
    } = await response(
      rewriteError('ast.rewrite.patch_limit', 'Patch exceeds limit', {
        terminalLimit: true,
      })
    );
    expect(row?.data).toMatchObject({ complete: false, isPartial: true });
    expect(row?.meta.diagnostics).toEqual({
      partial: true,
      codes: ['ast.rewrite.patch_limit', 'terminalLimitReached'],
    });
  });

  it.each([false, true])(
    'preserves partial-error pagination with continuation=%s',
    async continued => {
      const {
        results: [row],
      } = await response({
        status: 'error',
        errorCode: 'provider.partial',
        error: 'One collection could not be completed',
        pagination: { hasMore: true },
        ...(continued
          ? {
              next: {
                nextPage: {
                  tool: 'astRewrite',
                  query: {
                    path: '/repo',
                    langType: 'ts',
                    ruleKind: 'pattern',
                    pattern: 'old($A)',
                    rewrite: 'new($A)',
                    page: 2,
                  },
                },
              },
            }
          : {}),
      });
      expect(row?.status).toBe('error');
      expect(row?.meta.diagnostics).toEqual({
        partial: true,
        codes: continued
          ? ['provider.partial']
          : ['provider.partial', 'continuationMissing'],
      });
    }
  );
});
