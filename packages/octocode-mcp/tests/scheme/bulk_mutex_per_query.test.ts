import { describe, it, expect } from 'vitest';
import {
  BulkFetchContentQuerySchema,
  BulkRipgrepQuerySchema,
} from '../../src/scheme/localSchemaOverlay.js';
import { FileContentBulkQueryLocalSchema } from '../../src/scheme/remoteSchemaOverlay.js';

describe('bulk schemas defer mutex to per-query (no whole-batch rejection)', () => {
  it('localGetFileContent bulk accepts a mutex-violating query alongside valid ones', () => {
    const r = BulkFetchContentQuerySchema.safeParse({
      queries: [
        { path: 'a.ts', fullContent: true, matchString: 'x' },
        { path: 'b.ts', startLine: 1, endLine: 5 },
      ],
    });
    expect(r.success).toBe(true);
  });

  it('localSearchCode bulk accepts a mutex-violating query alongside valid ones', () => {
    const r = BulkRipgrepQuerySchema.safeParse({
      queries: [
        { pattern: 'x', path: '/r', filesOnly: true, filesWithoutMatch: true },
        { pattern: 'y', path: '/r' },
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
