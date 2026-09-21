import { describe, it, expect } from 'vitest';

import { LocalFetchContentQuerySchema } from '@octocodeai/config/schema';
import { LocalRipgrepQuerySchema } from '@octocodeai/config/schema';
import { FileContentQueryLocalSchema } from '@octocodeai/config/schema';
import { ArtifactSearchBulkQueryLocalSchema } from '@octocodeai/config/schema';

describe('LocalFetchContentQuerySchema mutual-exclusion', () => {
  const baseQuery = {
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

  it('rejects fullContent=true together with startLine/endLine', () => {
    const result = LocalFetchContentQuerySchema.safeParse({
      ...baseQuery,
      fullContent: true,
      startLine: 10,
      endLine: 20,
    });
    expect(result.success).toBe(false);
    if (!result.success) {
      const messages = result.error.issues.map(i => i.message).join('\n');
      expect(messages.toLowerCase()).toMatch(/startline\/endline/);
    }
  });

  it('rejects matchString together with startLine/endLine', () => {
    const result = LocalFetchContentQuerySchema.safeParse({
      ...baseQuery,
      matchString: 'foo',
      startLine: 10,
      endLine: 20,
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

  it('accepts startLine+endLine alone', () => {
    const result = LocalFetchContentQuerySchema.safeParse({
      ...baseQuery,
      startLine: 10,
      endLine: 20,
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

  it('rejects fullContent=true together with startLine', () => {
    const result = FileContentQueryLocalSchema.safeParse({
      ...baseQuery,
      fullContent: true,
      startLine: 10,
    });
    expect(result.success).toBe(false);
  });

  it('rejects fullContent=true together with endLine', () => {
    const result = FileContentQueryLocalSchema.safeParse({
      ...baseQuery,
      fullContent: true,
      endLine: 20,
    });
    expect(result.success).toBe(false);
  });

  it('rejects matchString together with startLine/endLine', () => {
    const result = FileContentQueryLocalSchema.safeParse({
      ...baseQuery,
      matchString: 'foo',
      startLine: 10,
      endLine: 20,
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

  it('accepts startLine+endLine alone', () => {
    const result = FileContentQueryLocalSchema.safeParse({
      ...baseQuery,
      startLine: 10,
      endLine: 20,
    });
    expect(result.success).toBe(true);
  });

  it('rejects an inverted startLine/endLine range', () => {
    const result = FileContentQueryLocalSchema.safeParse({
      ...baseQuery,
      startLine: 20,
      endLine: 10,
    });
    expect(result.success).toBe(false);
    if (!result.success) {
      const messages = result.error.issues.map(i => i.message).join('\n');
      expect(messages).toContain('endLine');
      expect(messages).toContain('startLine');
      expect(messages).toContain('greater than or equal');
      expect(messages.length).toBeLessThanOrEqual(90);
    }
  });
});

describe('LocalRipgrepQuerySchema enum contract', () => {
  // The old mutually-exclusive booleans (fixedString/perlRegex, filesOnly/
  // filesWithoutMatch, countLinesPerFile/countMatchesPerFile) were collapsed to
  // single enums (regex/output/unique), so those pairings are now impossible by
  // construction. These check the surviving cross-field gates instead.
  const baseQuery = {
    reasoning: 'exercise enum contract',
    searchText: 'foo',
    path: '/repo',
  };

  it('accepts the output enum values (files / filesWithout / count*)', () => {
    for (const output of [
      'content',
      'files',
      'filesWithout',
      'countLines',
      'countMatches',
    ] as const) {
      expect(
        LocalRipgrepQuerySchema.safeParse({ ...baseQuery, output }).success
      ).toBe(true);
    }
  });

  it('accepts the regex enum values (smart / fixed / perl)', () => {
    for (const regex of ['smart', 'fixed', 'perl'] as const) {
      expect(
        LocalRipgrepQuerySchema.safeParse({ ...baseQuery, regex }).success
      ).toBe(true);
    }
  });

  it('rejects unique without output:"matchOnly"', () => {
    const result = LocalRipgrepQuerySchema.safeParse({
      ...baseQuery,
      unique: 'list',
    });
    expect(result.success).toBe(false);
    if (!result.success) {
      const messages = result.error.issues.map(i => i.message).join('\n');
      expect(messages).toMatch(/unique requires output:"matchOnly"/);
    }
  });

  it('accepts unique:"count" with output:"matchOnly"', () => {
    const result = LocalRipgrepQuerySchema.safeParse({
      ...baseQuery,
      output: 'matchOnly',
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
      queries: [{ reasoning: 'exercise missing packageName' }],
    });
    expect(result.success).toBe(false);
  });
});
