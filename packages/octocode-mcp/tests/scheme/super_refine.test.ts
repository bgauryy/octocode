import { describe, it, expect } from 'vitest';

import { LocalFetchContentQuerySchema } from '@octocodeai/config/schema';
import { LocalSearchQuerySchema } from '@octocodeai/config/schema';
import { FileContentQueryLocalSchema } from '@octocodeai/config/schema';
import { ArtifactSearchBulkQueryLocalSchema } from '@octocodeai/config/schema';

describe('LocalFetchContentQuerySchema mutual-exclusion', () => {
  const baseQuery = {
    mainGoal: 'test goal',
    reasoning: 'exercise mutex validation',
    path: 'src/foo.ts',
  };

  it('rejects fullContent=true together with matchString', () => {
    const result = LocalFetchContentQuerySchema.safeParse({
      ...baseQuery,
      fullContent: true,
      matchString: 'foo',
    });
    expect(result.success).toBe(false);
    if (!result.success) {
      const messages = result.error.issues.map(i => i.message).join('\n');
      expect(messages.toLowerCase()).toMatch(/mutually exclusive|matchstring/);
    }
  });

  it('rejects fullContent=true together with ranges', () => {
    const result = LocalFetchContentQuerySchema.safeParse({
      ...baseQuery,
      fullContent: true,
      ranges: ['10-20'],
    });
    expect(result.success).toBe(false);
    if (!result.success) {
      const messages = result.error.issues.map(i => i.message).join('\n');
      expect(messages.toLowerCase()).toMatch(/ranges/);
    }
  });

  it('rejects matchString together with ranges', () => {
    const result = LocalFetchContentQuerySchema.safeParse({
      ...baseQuery,
      matchString: 'foo',
      ranges: ['10-20'],
    });
    expect(result.success).toBe(false);
    if (!result.success) {
      const messages = result.error.issues.map(i => i.message).join('\n');
      expect(messages.toLowerCase()).toMatch(/matchstring/);
    }
  });

  it('accepts fullContent=true alone', () => {
    const result = LocalFetchContentQuerySchema.safeParse({
      ...baseQuery,
      fullContent: true,
    });
    expect(result.success).toBe(true);
  });

  it('accepts matchString alone', () => {
    const result = LocalFetchContentQuerySchema.safeParse({
      ...baseQuery,
      matchString: 'foo',
    });
    expect(result.success).toBe(true);
  });

  it('accepts ranges alone', () => {
    const result = LocalFetchContentQuerySchema.safeParse({
      ...baseQuery,
      ranges: ['10-20'],
    });
    expect(result.success).toBe(true);
  });

  it('accepts fullContent=false with matchString', () => {
    const result = LocalFetchContentQuerySchema.safeParse({
      ...baseQuery,
      fullContent: false,
      matchString: 'foo',
    });
    expect(result.success).toBe(true);
  });
});

describe('FileContentQueryLocalSchema (github) three-mode mutual exclusion', () => {
  const baseQuery = {
    mainGoal: 'test goal',
    reasoning: 'exercise mutex validation',
    owner: 'o',
    repo: 'r',
    path: 'src/foo.ts',
  };

  it('rejects fullContent=true together with matchString', () => {
    const result = FileContentQueryLocalSchema.safeParse({
      ...baseQuery,
      fullContent: true,
      matchString: 'foo',
    });
    expect(result.success).toBe(false);
    if (!result.success) {
      const messages = result.error.issues.map(i => i.message).join('\n');
      expect(messages).toContain('fullContent');
      expect(messages).toContain('matchString');
      expect(messages.length).toBeLessThanOrEqual(90);
    }
  });

  it('rejects fullContent=true together with ranges', () => {
    const result = FileContentQueryLocalSchema.safeParse({
      ...baseQuery,
      fullContent: true,
      ranges: ['10-20'],
    });
    expect(result.success).toBe(false);
  });

  it('rejects matchString together with ranges', () => {
    const result = FileContentQueryLocalSchema.safeParse({
      ...baseQuery,
      matchString: 'foo',
      ranges: ['10-20'],
    });
    expect(result.success).toBe(false);
  });

  it('accepts fullContent=true alone', () => {
    const result = FileContentQueryLocalSchema.safeParse({
      ...baseQuery,
      fullContent: true,
    });
    expect(result.success).toBe(true);
  });

  it('accepts matchString alone', () => {
    const result = FileContentQueryLocalSchema.safeParse({
      ...baseQuery,
      matchString: 'foo',
    });
    expect(result.success).toBe(true);
  });

  it('accepts ranges alone', () => {
    const result = FileContentQueryLocalSchema.safeParse({
      ...baseQuery,
      ranges: ['10-20'],
    });
    expect(result.success).toBe(true);
  });

  it('rejects an inverted range', () => {
    const result = FileContentQueryLocalSchema.safeParse({
      ...baseQuery,
      ranges: ['20-10'],
    });
    expect(result.success).toBe(false);
    if (!result.success) {
      const messages = result.error.issues.map(i => i.message).join('\n');
      expect(messages).toContain('end >= start');
      expect(messages.length).toBeLessThanOrEqual(90);
    }
  });
});

describe('LocalSearchQuerySchema enum contract', () => {
  const baseQuery = {
    mainGoal: 'test goal',
    reasoning: 'exercise enum contract',
    matchString: 'foo',
    path: '/repo',
  };

  it('accepts the resultView enum values (files / filesWithout / count*)', () => {
    for (const resultView of [
      'content',
      'files',
      'filesWithout',
      'countLines',
      'countMatches',
    ] as const) {
      expect(
        LocalSearchQuerySchema.safeParse({ ...baseQuery, resultView }).success
      ).toBe(true);
    }
  });

  it('accepts the regex enum values (rust / literal / pcre2)', () => {
    for (const regex of ['rust', 'literal', 'pcre2'] as const) {
      expect(
        LocalSearchQuerySchema.safeParse({ ...baseQuery, regex }).success
      ).toBe(true);
    }
  });

  it('rejects unique without resultView:"matchOnly"', () => {
    const result = LocalSearchQuerySchema.safeParse({
      ...baseQuery,
      unique: 'list',
    });
    expect(result.success).toBe(false);
    if (!result.success) {
      const messages = result.error.issues.map(i => i.message).join('\n');
      expect(messages).toMatch(/requires? resultView:"matchOnly"/);
    }
  });

  it('accepts unique:"count" with resultView:"matchOnly"', () => {
    const result = LocalSearchQuerySchema.safeParse({
      ...baseQuery,
      resultView: 'matchOnly',
      unique: 'count',
    });
    expect(result.success).toBe(true);
  });
});

describe('ArtifactSearch schema', () => {
  it('accepts an exact packageName with ecosystem type', () => {
    const result = ArtifactSearchBulkQueryLocalSchema.safeParse({
      queries: [
        {
          mainGoal: 'test goal',
          reasoning: 'exercise artifact lookup',
          type: 'npm',
          packageName: 'react',
        },
      ],
    });
    expect(result.success).toBe(true);
  });

  it('rejects when packageName is missing', () => {
    const result = ArtifactSearchBulkQueryLocalSchema.safeParse({
      queries: [
        { mainGoal: 'test goal', reasoning: 'exercise missing packageName' },
      ],
    });
    expect(result.success).toBe(false);
  });
});
