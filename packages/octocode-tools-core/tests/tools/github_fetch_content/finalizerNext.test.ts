import { describe, expect, it } from 'vitest';
import { buildGithubFetchContentFinalizer } from '../../../src/tools/github_fetch_content/finalizer.js';
import type { FlatQueryResult } from '../../../src/types/toolResults.js';

type Query = {
  id: string;
  owner: string;
  repo: string;
  branch?: string;
  path: string;
  minify?: 'none' | 'standard' | 'symbols';
  type?: 'file' | 'directory';
};

function run(queries: Query[], results: FlatQueryResult[]) {
  const finalizer = buildGithubFetchContentFinalizer<Query>();
  return finalizer({ queries, results } as never);
}

describe('github fetch content finalizer next.continueChars', () => {
  it('emits a ready continuation query when char pagination hasMore', () => {
    const query: Query = {
      id: 'q1',
      owner: 'octo',
      repo: 'engine',
      branch: 'main',
      path: 'src/big.ts',
      minify: 'standard',
    };
    const result: FlatQueryResult = {
      id: 'q1',
      status: 'success',
      data: {
        path: 'src/big.ts',
        content: 'chunk-1',
        pagination: {
          currentPage: 1,
          totalPages: 3,
          hasMore: true,
          charOffset: 0,
          charLength: 2000,
          totalChars: 6000,
          nextCharOffset: 2000,
        },
      },
    };

    const out = run([query], [result]);
    const group = (
      out.structuredContent.results as Array<{
        files?: unknown[];
        data?: { files?: unknown[]; owner?: string; repo?: string };
      }>
    )[0]!;
    // Canonical shape: owner/repo/files live ONLY under data (no flat mirror).
    expect(group.data?.owner).toBe('octo');
    expect(group.data?.repo).toBe('engine');
    expect(group.files).toBeUndefined();

    const file = group.data?.files?.[0] as {
      next?: {
        continueChars?: { tool: string; query: Record<string, unknown> };
      };
    };

    expect(file.next?.continueChars).toEqual({
      tool: 'ghGetFileContent',
      query: {
        owner: 'octo',
        repo: 'engine',
        branch: 'main',
        path: 'src/big.ts',
        charOffset: 2000,
        charLength: 2000,
        minify: 'standard',
      },
    });
  });

  it('omits continueChars when there is no further page, but still offers the clone-for-semantics bridge (regression: this tool used to emit zero next-hints for a fully-read file)', () => {
    const query: Query = {
      id: 'q1',
      owner: 'octo',
      repo: 'engine',
      path: 'src/small.ts',
    };
    const result: FlatQueryResult = {
      id: 'q1',
      status: 'success',
      data: {
        path: 'src/small.ts',
        content: 'all',
        pagination: {
          currentPage: 1,
          totalPages: 1,
          hasMore: false,
          charOffset: 0,
          charLength: 3,
          totalChars: 3,
        },
      },
    };

    const out = run([query], [result]);
    const file = (
      out.structuredContent.results as Array<{ data?: { files?: unknown[] } }>
    )[0]?.data?.files?.[0] as {
      next?: {
        continueChars?: unknown;
        cloneForSemantics?: { tool: string; query: Record<string, unknown> };
      };
    };

    expect(file.next?.continueChars).toBeUndefined();
    expect(file.next?.cloneForSemantics).toEqual({
      tool: 'ghCloneRepo',
      query: { owner: 'octo', repo: 'engine', sparsePath: 'src/small.ts' },
      why: expect.stringContaining('lspGetSemantics'),
      confidence: 'exact',
    });
  });
});
