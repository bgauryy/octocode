import { describe, expect, it } from 'vitest';

import { buildGhSearchCodeFinalizer } from '../../../src/tools/github_search_code/finalizer/build.js';

type AnyRec = Record<string, unknown>;

function runFinalizer(queries: AnyRec[], results: AnyRec[]) {
  const finalize = buildGhSearchCodeFinalizer();
  const out = finalize({
    queries: queries as never,
    results: results as never,
    config: {} as never,
  });
  return out.structuredContent as AnyRec;
}

function groupResult(owner: string, repo: string, path: string, value: string) {
  return {
    id: `${owner}/${repo}`,
    owner,
    repo,
    matches: [{ path, value }],
  };
}

const pagination = (nextPage: number) => ({
  currentPage: nextPage - 1,
  totalPages: nextPage + 1,
  hasMore: true,
  nextPage,
});

describe('github.code finalizer — ordered bulk indexes are not merged', () => {
  it('emits one record per query with its own pagination', () => {
    const queries = [{}, {}];
    const results = [
      {
        index: 0,
        status: 'success',
        data: {
          results: [groupResult('octo', 'a', 'src/a.ts', 'foo')],
          pagination: pagination(2),
        },
      },
      {
        index: 1,
        status: 'success',
        data: {
          results: [groupResult('octo', 'b', 'src/b.ts', 'bar')],
          pagination: pagination(3),
        },
      },
    ];

    const sc = runFinalizer(queries, results);
    const records = sc.results as Array<{
      index: number;
      data: { pagination?: { nextPage?: number } };
    }>;
    expect(records.map(r => r.index)).toEqual([0, 1]);
    const byIndex = new Map(records.map(r => [r.index, r]));
    expect(byIndex.get(0)?.data.pagination?.nextPage).toBe(2);
    expect(byIndex.get(1)?.data.pagination?.nextPage).toBe(3);
  });
});

describe('github.code finalizer — row-local data.next continuation', () => {
  it('emits a ghGetFileContent matchString call for the top hit of a single query', () => {
    const queries = [{ keywords: ['createStoreImpl'] }];
    const results = [
      {
        index: 0,
        status: 'success',
        data: {
          results: [groupResult('pmndrs', 'zustand', 'src/vanilla.ts', 'x')],
        },
      },
    ];

    const sc = runFinalizer(queries, results);
    const next = (sc.results as Array<{ data: { next: unknown } }>)[0]!.data
      .next as Record<
      string,
      { tool: string; query: Record<string, unknown>; confidence?: string }
    >;
    expect(next.getLines).toBeDefined();
    expect(next.getLines!.tool).toBe('ghGetFileContent');
    expect(next.getLines!.query).toMatchObject({
      owner: 'pmndrs',
      repo: 'zustand',
      path: 'src/vanilla.ts',
      matchString: 'createStoreImpl',
    });
    expect(next.getLines!.confidence).toBe('low');
  });

  it('keeps each bulk query continuation with its ordered row', () => {
    const queries = [{ keywords: ['alpha'] }, { keywords: ['beta'] }];
    const results = [
      {
        index: 0,
        status: 'success',
        data: { results: [groupResult('o', 'a', 'a.ts', 'x')] },
      },
      {
        index: 1,
        status: 'success',
        data: { results: [groupResult('o', 'b', 'b.ts', 'y')] },
      },
    ];

    const sc = runFinalizer(queries, results);
    const rows = sc.results as Array<{
      data: { next: Record<string, { query: Record<string, unknown> }> };
    }>;
    expect(rows[0]!.data.next.getLines!.query.matchString).toBe('alpha');
    expect(rows[1]!.data.next.getLines!.query.matchString).toBe('beta');
  });

  it('maps repoState renamed → corrected retry continuation (warnings stripped)', () => {
    const queries = [
      {
        keywords: ['local.text'],
        owner: 'bgauryy',
        repo: 'octocode-mcp',
      },
    ];
    const results = [
      {
        index: 0,
        status: 'success',
        data: {
          results: [],
          repoState: { kind: 'renamed', fullName: 'bgauryy/octocode' },
        },
      },
    ];
    const sc = runFinalizer(queries, results);
    expect(sc.warnings).toBeUndefined();
    const row = (
      sc.results as Array<{
        data: { next: Record<string, { query: Record<string, unknown> }> };
      }>
    )[0]!;
    expect(row.data.next.retryRenamed!.query).toMatchObject({
      owner: 'bgauryy',
      repo: 'octocode',
      keywords: ['local.text'],
    });
    expect(row).toMatchObject({
      meta: {
        diagnostics: {
          codes: ['ghRepoRenamed'],
          hints: [expect.stringContaining('renamed')],
        },
      },
    });
  });

  it('maps repoState archived/notFound → empty rows with no warnings channel', () => {
    const queries = [
      { owner: 'octo', repo: 'archived' },
      { owner: 'octo', repo: 'missing' },
    ];
    const results = [
      {
        index: 0,
        status: 'success',
        data: { results: [], repoState: { kind: 'archived' } },
      },
      {
        index: 1,
        status: 'success',
        data: { results: [], repoState: { kind: 'notFound' } },
      },
    ];
    const sc = runFinalizer(queries, results);
    expect(sc.warnings).toBeUndefined();
    expect(sc.results).toMatchObject([
      {
        index: 0,
        status: 'empty',
        meta: { diagnostics: { codes: ['ghRepoArchived'] } },
        data: { next: { viewStructure: { tool: 'github.tree' } } },
      },
      {
        index: 1,
        status: 'empty',
        meta: { diagnostics: { codes: ['ghRepoNotFound'] } },
        data: { next: { findRepository: { tool: 'github.repositories' } } },
      },
    ]);
    expect(sc.emptyQueries).toBeUndefined();
  });

  it('keeps mixed batch failures as ordered row-local errors', () => {
    const queries = [{}, {}];
    const results = [
      {
        index: 0,
        status: 'error',
        data: { error: 'first query failed' },
      },
      {
        index: 1,
        status: 'success',
        data: { results: [groupResult('o', 'a', 'a.ts', 'x')] },
      },
    ];

    const sc = runFinalizer(queries, results);
    expect(sc.results).toMatchObject([
      { index: 0, status: 'error', data: { error: 'first query failed' } },
      { index: 1, data: { files: [{ path: 'a.ts' }] } },
    ]);
    expect(sc.errors).toBeUndefined();
  });

  it('omits next when there are no keywords to anchor on', () => {
    const queries = [{}];
    const results = [
      {
        index: 0,
        status: 'success',
        data: { results: [groupResult('o', 'a', 'a.ts', 'x')] },
      },
    ];
    const sc = runFinalizer(queries, results);
    expect(sc.next).toBeUndefined();
  });

  it('omits content-match continuation for path-only search', () => {
    const queries = [{ keywords: ['package.json'], match: 'path' }];
    const results = [
      {
        index: 0,
        status: 'success',
        data: {
          results: [groupResult('o', 'a', 'package.json', 'package.json')],
        },
      },
    ];
    const sc = runFinalizer(queries, results);
    expect(sc.next).toBeUndefined();
  });
});
