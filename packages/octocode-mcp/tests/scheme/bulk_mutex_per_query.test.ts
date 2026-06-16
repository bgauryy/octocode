import { describe, it, expect } from 'vitest';
import { LocalFetchContentBulkQuerySchema } from '../../../octocode-tools-core/src/tools/local_fetch_content/scheme.js';
import { LocalRipgrepBulkQuerySchema } from '../../../octocode-tools-core/src/tools/local_ripgrep/scheme.js';
import { FileContentBulkQueryLocalSchema } from '../../../octocode-tools-core/src/tools/github_fetch_content/scheme.js';

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

  it('ghGetFileContent bulk accepts a mutex-violating query alongside valid ones', () => {
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
