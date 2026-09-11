import { describe, expect, it } from 'vitest';
import { buildResponseChannels } from '../src/utils/response/responseChannels.js';
import { executeBulkOperation } from '../src/utils/response/bulk/response.js';
import { createErrorResult } from '../src/utils/response/error.js';

const continuation = {
  tool: 'localFetch',
  query: { path: '/tmp/source.ts', chunkType: 'bytes', offset: 4, limit: 4 },
  why: 'Continue reading the selected view.',
};

describe('public response hint policy', () => {
  it('removes success advice but preserves bounds, diagnostics, and executable continuations', () => {
    const { structuredContent, text } = buildResponseChannels(
      {
        results: [
          {
            index: 0,
            meta: {
              diagnostics: {
                partial: true,
                codes: ['bounded'],
                hints: ['Diagnostic advice'],
              },
            },
            data: {
              content: 'source',
              hints: ['Success advice'],
              isPartial: true,
              pagination: { hasMore: true },
              next: {
                fetch: continuation,
                continue: continuation,
                expandScan: continuation,
                verifyReferences: {
                  ...continuation,
                  tool: 'lspSearch',
                  query: {
                    uri: '/tmp/source.ts',
                    operation: 'references',
                    symbolName: 'source',
                    lineHint: 1,
                  },
                },
              },
              files: [
                {
                  path: 'source.ts',
                  hints: ['File advice'],
                  next: { readSite: continuation },
                },
              ],
            },
          },
        ],
      },
      []
    );
    const row = structuredContent.results[0]!;
    expect(row.data.next).toEqual({
      continue: { tool: continuation.tool, query: continuation.query },
      expandScan: { tool: continuation.tool, query: continuation.query },
      verifyReferences: {
        tool: 'lspSearch',
        query: {
          uri: '/tmp/source.ts',
          operation: 'references',
          symbolName: 'source',
          lineHint: 1,
        },
      },
    });
    expect(row.meta.diagnostics).toEqual({ partial: true, codes: ['bounded'] });
    expect(row.data.isPartial).toBe(true);
    expect(row.data.files).toEqual([{ path: 'source.ts' }]);
    expect(text).not.toMatch(/advice|why:/);
  });

  it('keeps one distinct short hint per empty/error row in a mixed batch', () => {
    const { structuredContent, text } = buildResponseChannels(
      {
        results: [
          { index: 0, data: { content: 'found', hints: ['Success advice'] } },
          {
            index: 1,
            status: 'empty',
            data: {
              hints: [
                ' Broaden   the query. ',
                'Broaden the query.',
                'Check the path.',
                'Third hint.',
              ],
              next: { fetch: continuation },
            },
          },
          {
            index: 2,
            status: 'error',
            meta: { diagnostics: { hints: ['Check credentials.'] } },
            data: {
              error: {
                error: 'Denied',
                hints: ['Check credentials.', 'Retry.'],
              },
              hints: ['Unneeded hint.'],
            },
          },
        ],
      },
      []
    );
    expect(structuredContent.results[0]!.data).toEqual({ content: 'found' });
    expect(structuredContent.results[1]!.data.hints).toEqual([
      'Broaden the query.',
    ]);
    expect(structuredContent.results[1]!.data.next).toEqual({
      fetch: continuation,
    });
    expect(text).not.toMatch(/Success advice|Third hint|Unneeded hint/);
  });

  it('bounds long recovery prose without altering executable queries or fetched evidence', () => {
    const query = { searchText: 'x'.repeat(300), options: {}, names: [] };
    const evidence = {
      hints: ['literal source field'],
      next: { fetch: 'literal data' },
    };
    const { structuredContent } = buildResponseChannels(
      {
        results: [
          {
            index: 0,
            data: {
              content: evidence,
              packages: [{ name: 'a', next: { cloneRepo: continuation } }],
              repositories: {
                a: { owner: 'a', next: { viewTree: continuation } },
              },
            },
          },
          {
            index: 1,
            status: 'error',
            data: {
              hints: ['A long explanation '.repeat(40)],
              next: {
                retry: {
                  tool: 'localSearch',
                  query,
                  why: 'Recovery explanation '.repeat(30),
                },
              },
            },
          },
        ],
      },
      []
    );
    expect(structuredContent.results[0]!.data.content).toEqual(evidence);
    expect(structuredContent.results[0]!.data.packages).toEqual([
      { name: 'a' },
    ]);
    expect(structuredContent.results[0]!.data.repositories).toEqual({
      a: { owner: 'a' },
    });
    const error = structuredContent.results[1]!.data;
    expect(error.hints![0]!.length).toBeLessThanOrEqual(120);
    expect(error.next!.retry.query).toEqual(query);
    expect(error.next!.retry.why.length).toBeLessThanOrEqual(120);
  });

  it.each([
    ['ghSearch', { operation: 'code' }, 'Broaden keywords or remove filters.'],
    [
      'ghGetFileContent',
      { path: 'src/index.ts' },
      'Verify owner/repo/branch/path, or remove matchString.',
    ],
    [
      'ghSearchHistory',
      { operation: 'commits' },
      'Broaden keywords or remove history filters.',
    ],
    [
      'ghGetHistoryItem',
      { operation: 'issue' },
      'Verify owner/repo and the number, ref, or compare refs.',
    ],
    [
      'artifactSearch',
      { type: 'npm' },
      'Check packageName, or broaden keywords.',
    ],
    [
      'ghCloneRepo',
      { owner: 'o', repo: 'r' },
      'Verify owner/repo/branch and sparsePath.',
    ],
    [
      'localSearch',
      { searchText: 'x' },
      'Broaden searchText, path, or filters.',
    ],
    [
      'astSearch',
      { operation: 'files' },
      'Broaden path or file filters.',
    ],
    [
      'localFetch',
      { path: '/tmp/x' },
      'Verify path/range, or remove matchString.',
    ],
    [
      'lspSearch',
      { operation: 'definition' },
      'Refresh uri/symbolName/lineHint, or broaden workspaceRoot.',
    ],
  ])('adds one minimal fallback for a bare %s empty row', (toolName, query, hint) => {
    const { structuredContent } = buildResponseChannels(
      { results: [{ index: 0, status: 'empty', data: {} }] },
      [],
      { toolName, queries: [query] }
    );
    expect(structuredContent.results[0]!.data.hints).toEqual([hint]);
    expect(hint.length).toBeLessThanOrEqual(120);
  });

  it('does not add a fallback when an error already gives an action', () => {
    const { structuredContent } = buildResponseChannels(
      {
        results: [
          {
            index: 0,
            status: 'error',
            data: { error: 'Directory matching requires langType; choose a grammar.' },
          },
        ],
      },
      [],
      { toolName: 'astSearch', queries: [{ operation: 'match' }] }
    );
    expect(structuredContent.results[0]!.data.hints).toBeUndefined();
  });

  it('does not add a hint when an executable recovery already exists', () => {
    const { structuredContent } = buildResponseChannels(
      {
        results: [
          {
            index: 0,
            status: 'empty',
            data: { next: { retry: continuation } },
          },
        ],
      },
      [],
      { toolName: 'localFetch', queries: [{ path: '/tmp/source.ts' }] }
    );
    expect(structuredContent.results[0]!.data.hints).toBeUndefined();
    expect(structuredContent.results[0]!.data.next).toBeDefined();
  });

  it('applies the tool-aware fallback through the bulk execution path', async () => {
    const result = await executeBulkOperation(
      [{ operation: 'files' }],
      async () => ({ status: 'empty', files: [] }),
      { toolName: 'astSearch' }
    );
    const row = (
      result.structuredContent as {
        results: Array<{ data: { hints?: string[] } }>;
      }
    ).results[0]!;
    expect(row.data.hints).toEqual(['Broaden path or file filters.']);
  });

  it('prefers an action over a hint that only restates the error', () => {
    const { structuredContent } = buildResponseChannels(
      {
        results: [
          {
            index: 0,
            status: 'error',
            data: {
              error: 'Symbol not found.',
              hints: [
                'Symbol "x" was not found near line 4.',
                'Run localSearch to refresh lineHint',
              ],
            },
          },
        ],
      },
      []
    );
    expect(structuredContent.results[0]!.data.hints).toEqual([
      'Run localSearch to refresh lineHint.',
    ]);
  });

  it('retains supplied recovery hints when creating an error', () => {
    expect(
      createErrorResult(
        'Denied',
        {},
        { extra: { hints: ['Check credentials.'] } }
      )
    ).toMatchObject({ status: 'error', hints: ['Check credentials.'] });
  });
});
