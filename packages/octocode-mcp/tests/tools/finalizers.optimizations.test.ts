/**
 * Unit tests for finalizer optimizations:
 *
 *  1. githubGetFileContent no longer pushes the
 *     "Partial content for ... ends at line N. Use startLine=N+1 to continue."
 *     hint to the top-level `hints[]` — that info is already on each file
 *     entry via isPartial/endLine/totalLines.
 *
 *  2. githubGetFileContent error hints fire on a GitHubAPIError-shaped failure
 *     even when the `.error` string was the generic "Provider error" — the
 *     finalizer must look at `status` (e.g. 404) too.
 *
 *  3. githubSearchCode emits a per-query "this query returned nothing" signal
 *     (`emptyQueries:[{id}]`) so callers can disambiguate a zero-match query
 *     from one that merged into another owner/repo group.
 *
 *  4. Truncator recovery string uses a straight apostrophe (not a curly one).
 */
import { describe, it, expect } from 'vitest';
import { buildGithubFetchContentFinalizer } from '../../src/tools/github_fetch_content/finalizer.js';
import { buildGithubSearchCodeFinalizer } from '../../src/tools/github_search_code/finalizer.js';
import type { FlatQueryResult } from '../../src/types.js';

describe('githubGetFileContent finalizer — optimization fixes', () => {
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
      config: { toolName: 'githubGetFileContent' },
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
    // Mirrors what handleProviderError places into FlatQueryResult.data when
    // upstream omits a textual reason but does carry HTTP status (e.g. 404).
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
      config: { toolName: 'githubGetFileContent' },
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

describe('githubSearchCode finalizer — optimization fixes', () => {
  it('FIX #3: surfaces zero-result queries via an emptyQueries[] signal', () => {
    const finalizer = buildGithubSearchCodeFinalizer();
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
      config: { toolName: 'githubSearchCode' },
    });

    expect(out.structuredContent.emptyQueries).toBeDefined();
    const ids = (out.structuredContent.emptyQueries ?? []).map(e => e.id);
    expect(ids).toContain('no_hits');
    expect(ids).not.toContain('has_hits');
  });

  it('FIX #3: omits emptyQueries[] entirely when every query had matches', () => {
    const finalizer = buildGithubSearchCodeFinalizer();
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
      config: { toolName: 'githubSearchCode' },
    });

    expect(out.structuredContent.emptyQueries).toBeUndefined();
  });

  it('ranks merged owner/repo groups by match count before pagination', () => {
    const finalizer = buildGithubSearchCodeFinalizer();
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
      config: { toolName: 'githubSearchCode' },
    });

    expect(out.structuredContent.results?.map(group => group.id)).toEqual([
      'org/large',
      'org/small',
    ]);
  });
});

describe('Truncator recovery strings — apostrophe consistency', () => {
  it('FIX #4: fetch_content truncator recovery uses a straight apostrophe', async () => {
    // Read the source file and ensure no curly apostrophe appears in the
    // truncator's recovery message. (We don't need to invoke the truncator —
    // the contract is a string-literal style guarantee.)
    const fs = await import('node:fs/promises');
    const path = await import('node:path');
    const file = path.resolve(
      __dirname,
      '../../src/tools/github_fetch_content/finalizer.ts'
    );
    const src = await fs.readFile(file, 'utf8');
    expect(src.includes('’')).toBe(false); // U+2019 RIGHT SINGLE QUOTATION MARK
  });
});
