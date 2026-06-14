import { describe, it, expect } from 'vitest';
import { LocalFetchContentBulkQuerySchema } from '@octocodeai/octocode-tools-core';
import { LocalRipgrepBulkQuerySchema } from '@octocodeai/octocode-tools-core';
import { FileContentBulkQueryLocalSchema } from '@octocodeai/octocode-tools-core';

describe('bulk schemas defer mutex to per-query (no whole-batch rejection)', () => {
  it('localGetFileContent bulk accepts a mutex-violating query alongside valid ones', () => {
    const r = LocalFetchContentBulkQuerySchema.safeParse({
      queries: [
        { path: 'a.ts', fullContent: true, matchString: 'x' },
        { path: 'b.ts', startLine: 1, endLine: 5 },
      ],
    });
    expect(r.success).toBe(true);
  });

  it('localSearchCode bulk accepts a mutex-violating query alongside valid ones', () => {
    const r = LocalRipgrepBulkQuerySchema.safeParse({
      queries: [
        { keywords: 'x', path: '/r', filesOnly: true, filesWithoutMatch: true },
        { keywords: 'y', path: '/r' },
      ],
    });
    expect(r.success).toBe(true);
  });

  it('githubGetFileContent bulk accepts a mutex-violating query alongside valid ones', () => {
    const r = FileContentBulkQueryLocalSchema.safeParse({
      queries: [
        {
          owner: 'o',
          repo: 'r',
          path: 'a.ts',
          fullContent: true,
          matchString: 'x',
        },
        { owner: 'o', repo: 'r', path: 'b.ts', startLine: 1, endLine: 5 },
      ],
    });
    expect(r.success).toBe(true);
  });
});
