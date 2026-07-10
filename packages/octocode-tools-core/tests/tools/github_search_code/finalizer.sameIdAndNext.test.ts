import { describe, expect, it } from 'vitest';

import { buildGhSearchCodeFinalizer } from '../../../src/tools/github_search_code/finalizer.js';
import { resolveUniqueQueryIds } from '../../../src/utils/response/bulk.js';

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

function groupResult(
  owner: string,
  repo: string,
  path: string,
  value: string
) {
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

describe('resolveUniqueQueryIds — duplicate explicit ids get suffixed', () => {
  it('keeps distinct ids untouched and suffixes collisions in order', () => {
    expect(
      resolveUniqueQueryIds([{ id: 'dup' }, { id: 'dup' }, { id: 'other' }])
    ).toEqual(['dup', 'dup#2', 'other']);
  });

  it('falls back to positional ids and never collides with them', () => {
    expect(resolveUniqueQueryIds([{}, { id: 'q1' }])).toEqual(['q1', 'q1#2']);
  });
});

describe('ghSearchCode finalizer — same-id bulk queries are not merged', () => {
  it('emits one record per query with its own pagination when ids collide upstream', () => {
    // bulk.ts now derives unique ids ('dup', 'dup#2') before the finalizer
    // runs; simulate its output for two queries submitted with the same id.
    const queries = [{ id: 'dup' }, { id: 'dup#2' }];
    const results = [
      {
        id: 'dup',
        status: 'success',
        data: {
          results: [groupResult('octo', 'a', 'src/a.ts', 'foo')],
          pagination: pagination(2),
        },
      },
      {
        id: 'dup#2',
        status: 'success',
        data: {
          results: [groupResult('octo', 'b', 'src/b.ts', 'bar')],
          pagination: pagination(3),
        },
      },
    ];

    const sc = runFinalizer(queries, results);
    const records = sc.results as Array<{
      id: string;
      data: { pagination?: { nextPage?: number } };
    }>;
    expect(records.map(r => r.id).sort()).toEqual(['dup', 'dup#2']);
    const byId = new Map(records.map(r => [r.id, r]));
    expect(byId.get('dup')?.data.pagination?.nextPage).toBe(2);
    expect(byId.get('dup#2')?.data.pagination?.nextPage).toBe(3);
  });
});

describe('ghSearchCode finalizer — next.getLines continuation', () => {
  it('emits a ghGetFileContent matchString call for the top hit of a single query', () => {
    const queries = [{ id: 'q1', keywords: ['createStoreImpl'] }];
    const results = [
      {
        id: 'q1',
        status: 'success',
        data: {
          results: [groupResult('pmndrs', 'zustand', 'src/vanilla.ts', 'x')],
        },
      },
    ];

    const sc = runFinalizer(queries, results);
    const next = sc.next as Record<
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
    expect(next.getLines!.confidence).toBe('heuristic');
  });

  it('emits per-query keys for multi-query bulk and uses each query own keyword', () => {
    const queries = [
      { id: 'q1', keywords: ['alpha'] },
      { id: 'q2', keywords: ['beta'] },
    ];
    const results = [
      {
        id: 'q1',
        status: 'success',
        data: { results: [groupResult('o', 'a', 'a.ts', 'x')] },
      },
      {
        id: 'q2',
        status: 'success',
        data: { results: [groupResult('o', 'b', 'b.ts', 'y')] },
      },
    ];

    const sc = runFinalizer(queries, results);
    const next = sc.next as Record<
      string,
      { query: Record<string, unknown> }
    >;
    expect(next['getLines:q1']!.query.matchString).toBe('alpha');
    expect(next['getLines:q2']!.query.matchString).toBe('beta');
  });

  it('maps repoState renamed → ghRepoRenamed diagnostic + corrected retry continuation', () => {
    const queries = [
      { id: 'q1', keywords: ['localSearchCode'], owner: 'bgauryy', repo: 'octocode-mcp' },
    ];
    const results = [
      {
        id: 'q1',
        status: 'success',
        data: {
          results: [],
          repoState: { kind: 'renamed', fullName: 'bgauryy/octocode' },
        },
      },
    ];
    const sc = runFinalizer(queries, results);
    const diags = sc.diagnostics as Array<{ code?: string; level: string }>;
    expect(diags?.some(d => d.code === 'ghRepoRenamed')).toBe(true);
    const next = sc.next as Record<string, { query: Record<string, unknown> }>;
    expect(next['retryRenamed:q1']!.query).toMatchObject({
      owner: 'bgauryy',
      repo: 'octocode',
      keywords: ['localSearchCode'],
    });
  });

  it('maps repoState archived/notFound → coded diagnostics', () => {
    const queries = [{ id: 'q1' }, { id: 'q2' }];
    const results = [
      {
        id: 'q1',
        status: 'success',
        data: { results: [], repoState: { kind: 'archived' } },
      },
      {
        id: 'q2',
        status: 'success',
        data: { results: [], repoState: { kind: 'notFound' } },
      },
    ];
    const sc = runFinalizer(queries, results);
    const codes = (sc.diagnostics as Array<{ code?: string }>).map(d => d.code);
    expect(codes).toContain('ghRepoArchived');
    expect(codes).toContain('ghRepoNotFound');
  });

  it('omits next when there are no keywords to anchor on', () => {
    const queries = [{ id: 'q1' }];
    const results = [
      {
        id: 'q1',
        status: 'success',
        data: { results: [groupResult('o', 'a', 'a.ts', 'x')] },
      },
    ];
    const sc = runFinalizer(queries, results);
    expect(sc.next).toBeUndefined();
  });
});
