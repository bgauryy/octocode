import { describe, expect, it } from 'vitest';

import { DirectToolInputError } from '../../src/tools/directToolCatalog/toolCatalogDefinitions.js';
import { prepareDirectToolInput } from '../../src/tools/directToolCatalog/toolInputPreparation.js';

const removedAliases: ReadonlyArray<
  readonly [tool: string, canonical: string, query: Record<string, unknown>]
> = [
  ['npmSearch', 'packageName', { name: 'zod' }],
  ['lspGetSemantics', 'type', { op: 'documentSymbols', uri: '/repo/a.ts' }],
  ['localGetFileContent', 'path', { filePath: '/repo/a.ts' }],
  [
    'localSearch',
    'searchText',
    { operation: 'text', path: '/repo', keywords: 'needle' },
  ],
  [
    'localSearch',
    'langType',
    {
      operation: 'structural',
      path: '/repo',
      pattern: 'call($X)',
      language: 'typescript',
    },
  ],
  [
    'localSearch',
    'pageSize',
    { operation: 'text', path: '/repo', searchText: 'x', itemsPerPage: 5 },
  ],
  [
    'localSearch',
    'sort',
    { operation: 'files', path: '/repo', sortBy: 'name' },
  ],
  [
    'localSearch',
    'reverse',
    { operation: 'text', path: '/repo', searchText: 'x', sortReverse: true },
  ],
  [
    'ghGetFileContent',
    'contextLines',
    {
      owner: 'o',
      repo: 'r',
      path: 'a.ts',
      matchString: 'x',
      matchStringContextLines: 2,
    },
  ],
  [
    'ghGetFileContent',
    'minify',
    { owner: 'o', repo: 'r', path: 'a.ts', minified: true },
  ],
  [
    'ghSearchHistory',
    'merged',
    { operation: 'pullRequests', owner: 'o', repo: 'r', merged: true },
  ],
  [
    'ghSearchHistory',
    'keywordsToSearch',
    {
      operation: 'pullRequests',
      owner: 'o',
      repo: 'r',
      keywordsToSearch: ['x'],
    },
  ],
  [
    'ghSearchHistory',
    'keywordsToSearch',
    {
      operation: 'issues',
      owner: 'o',
      repo: 'r',
      keywordsToSearch: ['x'],
    },
  ],
  [
    'ghSearch',
    'topics',
    { operation: 'repositories', topicsToSearch: ['mcp'] },
  ],
  [
    'ghSearch',
    'pageSize',
    { operation: 'tree', owner: 'o', repo: 'r', itemsPerPage: 5 },
  ],
  [
    'ghGetHistoryItem',
    'filePath',
    {
      operation: 'commit',
      owner: 'o',
      repo: 'r',
      ref: 'abc',
      filePath: 'src/a.ts',
    },
  ],
  [
    'localAnalyzeGraph',
    'depth',
    { path: '/repo', operation: 'dependencies', file: 'src/a.ts', maxDepth: 2 },
  ],
  [
    'localAnalyzeGraph',
    'pageSize',
    { path: '/repo', operation: 'cycles', itemsPerPage: 5 },
  ],
];

describe('canonical direct-tool inputs', () => {
  it.each(removedAliases)(
    '%s rejects a removed alias and names %s',
    (tool, canonical, query) => {
      expect(() =>
        prepareDirectToolInput(tool, query, { rejectUnknownFields: true })
      ).toThrowError(DirectToolInputError);

      try {
        prepareDirectToolInput(tool, query, { rejectUnknownFields: true });
      } catch (error) {
        expect(error).toBeInstanceOf(DirectToolInputError);
        const details = (error as DirectToolInputError).details.join(' ');
        expect(details).toContain(canonical);
      }
    }
  );

  it('does not reinterpret text pattern as searchText', () => {
    expect(() =>
      prepareDirectToolInput(
        'localSearch',
        { operation: 'text', path: '/repo', pattern: 'needle' },
        { rejectUnknownFields: true }
      )
    ).toThrowError(DirectToolInputError);
  });

  it('accepts only canonical entryType values', () => {
    expect(() =>
      prepareDirectToolInput(
        'localSearch',
        { operation: 'files', path: '/repo', entryType: 'file' },
        { rejectUnknownFields: true }
      )
    ).toThrowError(DirectToolInputError);

    expect(
      prepareDirectToolInput(
        'localSearch',
        { operation: 'files', path: '/repo', entryType: 'f' },
        { rejectUnknownFields: true }
      )
    ).toMatchObject({ queries: [{ entryType: 'f' }] });
  });

  it.each([
    [
      'ghSearchPullRequests',
      { owner: 'o', repo: 'r', prNumber: 1, reviewMode: 'full' },
      'review',
    ],
    ['ghSearch', { operation: 'code', keywords: ['x'], limit: 10 }, 'pageSize'],
  ])(
    'does not suggest a non-equivalent field for %s',
    (tool, query, wrongHint) => {
      try {
        prepareDirectToolInput(tool, query, { rejectUnknownFields: true });
        expect.unreachable('expected rejected input');
      } catch (error) {
        expect(error).toBeInstanceOf(DirectToolInputError);
        expect((error as DirectToolInputError).details.join(' ')).not.toContain(
          `did you mean '${wrongHint}'`
        );
      }
    }
  );
});
