import { describe, it, expect } from 'vitest';
import {
  LOCAL_MAX_DEPTH,
  LOCAL_MAX_LIMIT,
  LspSearchQuerySchema,
  StructureFilesQuerySchema,
  StructureTreeQuerySchema,
} from '@octocodeai/config/schema';

describe.each([
  ['files', StructureFilesQuerySchema],
  ['tree', StructureTreeQuerySchema],
] as const)('structureSearch %s maxEntries bounds', (operation, schema) => {
  it.each([-5, LOCAL_MAX_LIMIT + 1])(
    'rejects out-of-range maxEntries %s without clamping',
    maxEntries => {
      expect(
        schema.safeParse({
          mainGoal: 'test goal',
          reasoning: 'exercise scan-cap bounds',
          operation,
          path: '.',
          maxEntries,
        }).success
      ).toBe(false);
    }
  );

  it('accepts maxEntries at the maximum', () => {
    const result = schema.safeParse({
      mainGoal: 'test goal',
      reasoning: 'exercise scan-cap bounds',
      operation,
      path: '.',
      maxEntries: LOCAL_MAX_LIMIT,
    });
    expect(result.success).toBe(true);
    if (result.success) expect(result.data.maxEntries).toBe(LOCAL_MAX_LIMIT);
  });

  it('accepts an omitted maxEntries', () => {
    expect(
      schema.safeParse({
        mainGoal: 'test goal',
        reasoning: 'exercise scan-cap bounds',
        operation,
        path: '.',
      }).success
    ).toBe(true);
  });
});

describe('structureSearch tree depth bounds', () => {
  it.each([0, LOCAL_MAX_DEPTH + 1])(
    'rejects out-of-range depth %s without clamping',
    maxDepth => {
      expect(
        StructureTreeQuerySchema.safeParse({
          mainGoal: 'test goal',
          reasoning: 'exercise depth bounds',
          operation: 'tree',
          path: '.',
          maxDepth,
        }).success
      ).toBe(false);
    }
  );

  it.each([1, LOCAL_MAX_DEPTH])(
    'accepts depth at the boundary %s',
    maxDepth => {
      const result = StructureTreeQuerySchema.safeParse({
        mainGoal: 'test goal',
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
    mainGoal: 'test goal',
    reasoning: 'exercise depth bounds',
    path: '/tmp/x.ts',
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
      expect(result.data).toMatchObject({ depth: LOCAL_MAX_DEPTH });
    }
  });
});
