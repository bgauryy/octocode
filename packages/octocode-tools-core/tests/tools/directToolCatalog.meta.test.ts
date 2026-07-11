import { describe, expect, it } from 'vitest';

import {
  DirectToolInputError,
  prepareDirectToolInput,
} from '../../src/tools/directToolCatalog.meta.js';

describe('prepareDirectToolInput', () => {
  it('rejects unknown query fields when strict mode is enabled', () => {
    expect(() =>
      prepareDirectToolInput(
        'localSearchCode',
        { path: '.', keywords: 'runCLI', typo: true },
        { rejectUnknownFields: true }
      )
    ).toThrow(DirectToolInputError);

    expect(() =>
      prepareDirectToolInput(
        'localSearchCode',
        { path: '.', keywords: 'runCLI', typo: true },
        { rejectUnknownFields: true }
      )
    ).toThrow('Unknown field(s): typo');
  });

  it('folds well-known cross-tool field renames to the canonical field (no error)', () => {
    const cases: Array<{
      tool: string;
      query: Record<string, unknown>;
      canonical: string;
      alias: string;
    }> = [
      {
        tool: 'ghSearchCode',
        query: { keywordsToSearch: ['x'], owner: 'o', repo: 'r' },
        alias: 'keywordsToSearch',
        canonical: 'keywords',
      },
      {
        tool: 'ghSearchRepos',
        query: { keywordsToSearch: ['octocode'], concise: true, limit: 3 },
        alias: 'keywordsToSearch',
        canonical: 'keywords',
      },
      {
        tool: 'ghViewRepoStructure',
        query: { owner: 'o', repo: 'r', path: '', depth: 1 },
        alias: 'depth',
        canonical: 'maxDepth',
      },
      {
        tool: 'npmSearch',
        query: { name: 'zod' },
        alias: 'name',
        canonical: 'packageName',
      },
      {
        tool: 'lspGetSemantics',
        query: { path: '/tmp/x.ts', line: 12, op: 'references' },
        alias: 'line',
        canonical: 'lineHint',
      },
    ];
    for (const { tool, query, alias, canonical } of cases) {
      const prepared = prepareDirectToolInput(tool, query, {
        rejectUnknownFields: true,
      }) as { queries: Array<Record<string, unknown>> };
      const first = prepared.queries[0]!;
      expect(first[canonical], `${tool}.${canonical}`).toEqual(
        query[alias as keyof typeof query]
      );
      expect(first[alias], `${tool}.${alias} removed`).toBeUndefined();
    }
  });

  it('still suggests the closest field for real typos, but not for short unknowns', () => {
    try {
      prepareDirectToolInput(
        'ghSearchCode',
        { keywordz: ['x'], owner: 'o', repo: 'r' },
        { rejectUnknownFields: true }
      );
      expect.unreachable('expected ghSearchCode to reject unknown fields');
    } catch (error) {
      expect(error).toBeInstanceOf(DirectToolInputError);
      const details = (error as DirectToolInputError & { details?: string[] })
        .details;
      expect(details).toContain("'keywordz' → did you mean 'keywords'?");
    }

    // 2-char unknowns must not get fuzzy false friends ('xq' ≈ 'id' etc.).
    try {
      prepareDirectToolInput(
        'ghSearchCode',
        { xq: 1, owner: 'o', repo: 'r' },
        { rejectUnknownFields: true }
      );
      expect.unreachable('expected ghSearchCode to reject unknown fields');
    } catch (error) {
      const details = (error as DirectToolInputError & { details?: string[] })
        .details;
      expect(details?.some(d => d.includes('did you mean'))).toBe(false);
    }
  });

  it('keeps ghSearchRepos canonical keywords (does not rewrite to keywordsToSearch)', () => {
    const prepared = prepareDirectToolInput(
      'ghSearchRepos',
      { keywords: ['octocode'], concise: true, limit: 3 },
      { rejectUnknownFields: true }
    ) as { queries: Array<Record<string, unknown>> };
    const first = prepared.queries[0]!;
    expect(first.keywords).toEqual(['octocode']);
    expect(first.keywordsToSearch).toBeUndefined();
  });
});
