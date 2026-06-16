import { describe, it, expect } from 'vitest';
import { buildGithubFetchContentFinalizer } from '../../../octocode-tools-core/src/tools/github_fetch_content/finalizer.js';
import { buildGhSearchCodeFinalizer } from '../../../octocode-tools-core/src/tools/github_search_code/finalizer.js';
import type { FlatQueryResult } from '../../../octocode-tools-core/src/types/toolResults.js';

describe('ghGetFileContent finalizer — optimization fixes', () => {
  it('FIX #1: does not emit top-level "Partial content ... Use startLine=..." hint when the file is partial (info already in fields)', () => {
    const finalizer = buildGithubFetchContentFinalizer();
    const queries = [
      {
        id: 'q1',
        owner: 'facebook',
        repo: 'react',
        path: 'README.md',
      },
    ];
    const results: FlatQueryResult[] = [
      {
        id: 'q1',
        data: {
          path: 'README.md',
          content: '# React',
          totalLines: 79,
          isPartial: true,
          startLine: 1,
          endLine: 4,
        },
      },
    ];

    const out = finalizer({
      queries,
      results,
      config: { toolName: 'ghGetFileContent' },
    });

    const file = out.structuredContent.results[0]?.files?.[0];
    expect(file?.isPartial).toBe(true);
    expect(file?.totalLines).toBe(79);
    expect(file?.endLine).toBe(4);

    const hints = out.structuredContent.hints ?? [];
    expect(
      hints.some(h => /^Partial content for .* Use startLine=/.test(h))
    ).toBe(false);
  });

  it('FIX #2: emits dynamic 404 hints when the failure carries an HTTP status, even with a generic error string', () => {
    const finalizer = buildGithubFetchContentFinalizer();
    const queries = [
      {
        id: 'q1',
        owner: 'facebook',
        repo: 'react',
        path: 'this_file_does_not_exist.md',
      },
    ];
    const results: FlatQueryResult[] = [
      {
        id: 'q1',
        status: 'error',
        data: {
          error: {
            error: 'Provider error',
            type: 'http',
            status: 404,
          },
        },
      },
    ];

    const out = finalizer({
      queries,
      results,
      config: { toolName: 'ghGetFileContent' },
    });

    const errors = out.structuredContent.errors;
    expect(errors).toBeDefined();
    expect(errors).toHaveLength(1);
    expect(errors?.[0]?.hints).toBeDefined();
    const hintBlob = (errors?.[0]?.hints ?? []).join(' | ');
    expect(hintBlob.toLowerCase()).toMatch(
      /owner|repo|path|branch|not found|verify/
    );
  });
});

describe('ghSearchCode finalizer — optimization fixes', () => {
  it('FIX #3: surfaces zero-result queries via an emptyQueries[] signal', () => {
    const finalizer = buildGhSearchCodeFinalizer();
    const queries = [
      {
        id: 'has_hits',
        keywordsToSearch: ['useState'],
      },
      {
        id: 'no_hits',
        keywordsToSearch: ['ZZZZ_does_not_exist_zzzz'],
      },
    ];
    const results: FlatQueryResult[] = [
      {
        id: 'has_hits',
        data: {
          results: [
            {
              id: 'facebook/react',
              owner: 'facebook',
              repo: 'react',
              matches: [{ path: 'packages/react/index.js' }],
            },
          ],
        },
      },
      {
        id: 'no_hits',
        status: 'empty',
        data: { results: [] },
      },
    ];

    const out = finalizer({
      queries,
      results,
      config: { toolName: 'ghSearchCode' },
    });

    expect(out.structuredContent.emptyQueries).toBeDefined();
    const ids = (out.structuredContent.emptyQueries ?? []).map(e => e.id);
    expect(ids).toContain('no_hits');
    expect(ids).not.toContain('has_hits');
  });

  it('FIX #3: omits emptyQueries[] entirely when every query had matches', () => {
    const finalizer = buildGhSearchCodeFinalizer();
    const queries = [{ id: 'q1', keywordsToSearch: ['useState'] }];
    const results: FlatQueryResult[] = [
      {
        id: 'q1',
        data: {
          results: [
            {
              id: 'facebook/react',
              owner: 'facebook',
              repo: 'react',
              matches: [{ path: 'a.js' }],
            },
          ],
        },
      },
    ];

    const out = finalizer({
      queries,
      results,
      config: { toolName: 'ghSearchCode' },
    });

    expect(out.structuredContent.emptyQueries).toBeUndefined();
  });

  it('ranks merged owner/repo groups by match count before pagination', () => {
    const finalizer = buildGhSearchCodeFinalizer();
    const queries = [{ id: 'q1', keywordsToSearch: ['handler'] }];
    const results: FlatQueryResult[] = [
      {
        id: 'q1',
        data: {
          results: [
            {
              id: 'org/small',
              owner: 'org',
              repo: 'small',
              matches: [{ path: 'a.ts' }],
            },
            {
              id: 'org/large',
              owner: 'org',
              repo: 'large',
              matches: [{ path: 'b.ts' }, { path: 'c.ts' }, { path: 'd.ts' }],
            },
          ],
        },
      },
    ];

    const out = finalizer({
      queries,
      results,
      config: { toolName: 'ghSearchCode' },
    });

    const files = out.structuredContent.results?.[0]?.data.files ?? [];
    expect(files.map(file => file.repo)).toEqual([
      'large',
      'large',
      'large',
      'small',
    ]);
    expect(files[0]?.matches[0]).not.toHaveProperty('path');
  });
});

describe('Truncator recovery strings — apostrophe consistency', () => {
  it('FIX #4: fetch_content truncator recovery uses a straight apostrophe', async () => {
    const fs = await import('node:fs/promises');
    const path = await import('node:path');
    const file = path.resolve(
      __dirname,
      '../../../octocode-tools-core/src/tools/github_fetch_content/finalizer.ts'
    );
    const src = await fs.readFile(file, 'utf8');
    expect(src.includes('’')).toBe(false);
  });
});
