import { describe, it, expect } from 'vitest';
import {
  AstFilesQuerySchema,
  AstFilesystemTreeQuerySchema,
  LOCAL_MAX_DEPTH,
  LOCAL_MAX_LIMIT,
  LspSearchQuerySchema,
} from '@octocodeai/config/schema';

describe.each([
  ['files', AstFilesQuerySchema],
  ['tree', AstFilesystemTreeQuerySchema],
] as const)('astSearch %s limit bounds', (operation, schema) => {
  it.each([-5, LOCAL_MAX_LIMIT + 1])(
    'rejects out-of-range limit %s without clamping',
    limit => {
      expect(
        schema.safeParse({
          reasoning: 'exercise limit bounds',
          operation,
          path: '.',
          limit,
        }).success
      ).toBe(false);
    }
  );

  it('accepts limit at the maximum', () => {
    const result = schema.safeParse({
      reasoning: 'exercise limit bounds',
      operation,
      path: '.',
      limit: LOCAL_MAX_LIMIT,
    });
    expect(result.success).toBe(true);
    if (result.success) expect(result.data.limit).toBe(LOCAL_MAX_LIMIT);
  });

  it('accepts an omitted limit', () => {
    expect(
      schema.safeParse({
        reasoning: 'exercise limit bounds',
        operation,
        path: '.',
      }).success
    ).toBe(true);
  });
});

describe('astSearch filesystem tree depth bounds', () => {
  it.each([-1, LOCAL_MAX_DEPTH + 1])(
    'rejects out-of-range depth %s without clamping',
    maxDepth => {
      expect(
        AstFilesystemTreeQuerySchema.safeParse({
          reasoning: 'exercise depth bounds',
          operation: 'tree',
          path: '.',
          maxDepth,
        }).success
      ).toBe(false);
    }
  );

  it.each([0, LOCAL_MAX_DEPTH])(
    'accepts depth at the boundary %s',
    maxDepth => {
      const result = AstFilesystemTreeQuerySchema.safeParse({
        reasoning: 'exercise depth bounds',
        operation: 'tree',
        path: '.',
        maxDepth,
      });
      expect(result.success).toBe(true);
      if (result.success) expect(result.data.maxDepth).toBe(maxDepth);
    }
  );
});

describe('LspSearchQuerySchema depth bound', () => {
  const base = {
    reasoning: 'exercise depth bounds',
    uri: '/tmp/x.ts',
    operation: 'callers',
    symbolName: 'x',
    lineHint: 1,
  };

  it('rejects depth above the advertised maximum', () => {
    const result = LspSearchQuerySchema.safeParse({
      ...base,
      depth: LOCAL_MAX_DEPTH + 1,
    });
    expect(result.success).toBe(false);
  });

  it('rejects negative depth', () => {
    const result = LspSearchQuerySchema.safeParse({
      ...base,
      depth: -1,
    });
    expect(result.success).toBe(false);
  });

  it('accepts depth at the max bound', () => {
    const result = LspSearchQuerySchema.safeParse({
      ...base,
      depth: LOCAL_MAX_DEPTH,
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.depth).toBe(LOCAL_MAX_DEPTH);
    }
  });
});
