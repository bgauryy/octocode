import { describe, it, expect } from 'vitest';
import { LocalFetchContentBulkQuerySchema } from '@octocodeai/config/schema';
import { AstSearchBulkQuerySchema } from '@octocodeai/config/schema';
import { FileContentBulkQueryLocalSchema } from '@octocodeai/config/schema';

describe('bulk schema cross-field validation', () => {
  it('localFetch rejects a mutex-violating row in a mixed batch', () => {
    const r = LocalFetchContentBulkQuerySchema.safeParse({
      queries: [
        {
          goal: 'test goal',
          reasoning: 'exercise bulk validation',
          path: 'a.ts',
          fullContent: true,
          matchString: 'x',
        },
        {
          goal: 'test goal',
          reasoning: 'exercise bulk validation',
          path: 'b.ts',
          startLine: 1,
          endLine: 5,
        },
      ],
    });
    expect(r.success).toBe(false);
    expect(
      LocalFetchContentBulkQuerySchema.safeParse({
        queries: [
          {
            goal: 'test goal',
            reasoning: 'exercise bulk validation',
            path: 'b.ts',
            startLine: 1,
            endLine: 5,
          },
        ],
      }).success
    ).toBe(true);
  });

  it('astSearch rejects pattern and rule together in a mixed batch', () => {
    const r = AstSearchBulkQuerySchema.safeParse({
      queries: [
        {
          goal: 'test goal',
          reasoning: 'exercise bulk validation',
          operation: 'match',
          path: '/r',
          langType: 'ts',
          pattern: 'call($A)',
          rule: 'kind: call_expression',
        },
        {
          goal: 'test goal',
          reasoning: 'exercise bulk validation',
          operation: 'files',
          path: '/r',
        },
      ],
    });
    expect(r.success).toBe(false);
    if (!r.success) {
      const serialized = JSON.stringify(r.error.issues);
      expect(serialized).toContain('pattern');
      expect(serialized).toContain('rule');
    }
    expect(
      AstSearchBulkQuerySchema.safeParse({
        queries: [
          {
            goal: 'test goal',
            reasoning: 'exercise bulk validation',
            operation: 'match',
            path: '/r',
            langType: 'ts',
            pattern: 'call($A)',
          },
        ],
      }).success
    ).toBe(true);
  });

  it('ghGetFileContent rejects a mutex-violating row in a mixed batch', () => {
    const r = FileContentBulkQueryLocalSchema.safeParse({
      queries: [
        {
          goal: 'test goal',
          reasoning: 'exercise bulk validation',
          owner: 'o',
          repo: 'r',
          path: 'a.ts',
          fullContent: true,
          matchString: 'x',
        },
        {
          goal: 'test goal',
          reasoning: 'exercise bulk validation',
          owner: 'o',
          repo: 'r',
          path: 'b.ts',
          startLine: 1,
          endLine: 5,
        },
      ],
    });
    expect(r.success).toBe(false);
    expect(
      FileContentBulkQueryLocalSchema.safeParse({
        queries: [
          {
            goal: 'test goal',
            reasoning: 'exercise bulk validation',
            owner: 'o',
            repo: 'r',
            path: 'b.ts',
            startLine: 1,
            endLine: 5,
          },
        ],
      }).success
    ).toBe(true);
  });
});
