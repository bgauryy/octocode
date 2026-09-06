import { beforeEach, describe, expect, it, vi } from 'vitest';
import { buildGithubFetchContentFinalizer } from '../../../src/tools/github_fetch_content/finalizer.js';
import type { FlatQueryResult } from '../../../src/types/toolResults.js';

// cloneForSemantics must only be offered when clone is actually enabled —
// otherwise the hint names a tool (ghCloneRepo) that isn't registered.
const mockIsCloneEnabled = vi.fn<() => boolean>(() => true);
vi.mock('../../../src/serverConfig.js', async importOriginal => ({
  ...(await importOriginal<typeof import('../../../src/serverConfig.js')>()),
  isCloneEnabled: () => mockIsCloneEnabled(),
}));

type Query = {
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
  beforeEach(() => {
    mockIsCloneEnabled.mockReturnValue(true);
  });

  it('uses the unified tree operation in not-found recovery guidance', () => {
    const out = run(
      [{ owner: 'octo', repo: 'engine', path: 'Src/missing.ts' }],
      [
        {
          index: 0,
          status: 'error',
          data: { error: '404 not found' },
        },
      ]
    );
    const row = (
      out.structuredContent.results as Array<{ data?: { error?: string } }>
    )[0];
    expect(row?.data?.error).toContain('ghSearch with operation:"tree"');
    expect(row?.data?.error).not.toContain('github.tree');
  });

  it('preserves a structured provider error message', () => {
    const out = run(
      [{ owner: 'octo', repo: 'engine', path: 'README.md' }],
      [
        {
          index: 0,
          status: 'error',
          data: {
            error: {
              error: 'Request timed out while contacting GitHub.',
              type: 'network',
            },
          },
        },
      ]
    );
    const row = (
      out.structuredContent.results as Array<{ data?: { error?: string } }>
    )[0];
    expect(row?.data?.error).toBe('Request timed out while contacting GitHub.');
  });

  it('emits a ready continuation query when char pagination hasMore', () => {
    const query: Query = {
      owner: 'octo',
      repo: 'engine',
      branch: 'main',
      path: 'src/big.ts',
      minify: 'standard',
      matchString: 'needle',
      matchStringIsRegex: false,
      matchStringCaseSensitive: true,
      contextLines: 3,
    };
    const result: FlatQueryResult = {
      index: 0,
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
        matchString: 'needle',
        matchStringIsRegex: false,
        matchStringCaseSensitive: true,
        contextLines: 3,
        charOffset: 2000,
        charLength: 2000,
        minify: 'standard',
      },
    });
  });

  it('preserves a line selector and does not skip to the next line range before char pages finish', () => {
    const query: Query = {
      owner: 'octo',
      repo: 'engine',
      path: 'src/big.ts',
      startLine: 1,
      endLine: 20,
      charLength: 10,
    };
    const result: FlatQueryResult = {
      index: 0,
      status: 'success',
      data: {
        path: 'src/big.ts',
        content: 'chunk-1',
        startLine: 1,
        endLine: 20,
        totalLines: 100,
        pagination: {
          currentPage: 1,
          totalPages: 3,
          hasMore: true,
          charOffset: 0,
          charLength: 10,
          totalChars: 30,
          nextCharOffset: 10,
        },
      },
    };

    const out = run([query], [result]);
    const file = (
      out.structuredContent.results as Array<{ data?: { files?: unknown[] } }>
    )[0]?.data?.files?.[0] as {
      next?: Record<string, { tool?: string; query?: Record<string, unknown> }>;
    };
    expect(file.next?.continueChars?.query).toMatchObject({
      startLine: 1,
      endLine: 20,
      charOffset: 10,
      charLength: 10,
    });
    expect(file.next?.continueLines).toBeUndefined();
  });

  it('omits continueChars when there is no further page, but still offers the clone-for-semantics bridge (regression: this tool used to emit zero next-hints for a fully-read file)', () => {
    const query: Query = {
      owner: 'octo',
      repo: 'engine',
      path: 'src/small.ts',
    };
    const result: FlatQueryResult = {
      index: 0,
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
    expect(JSON.stringify(file.next)).not.toContain('local.text');
  });

  it('omits cloneForSemantics (and an empty next map entirely) when clone is disabled', () => {
    mockIsCloneEnabled.mockReturnValue(false);
    const query: Query = {
      owner: 'octo',
      repo: 'engine',
      path: 'src/small.ts',
    };
    const result: FlatQueryResult = {
      index: 0,
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
    )[0]?.data?.files?.[0] as { next?: Record<string, unknown> };

    expect(file.next).toBeUndefined();
  });

  it('keeps continueChars when clone is disabled but more pages exist', () => {
    mockIsCloneEnabled.mockReturnValue(false);
    const query: Query = {
      owner: 'octo',
      repo: 'engine',
      path: 'src/big.ts',
    };
    const result: FlatQueryResult = {
      index: 0,
      status: 'success',
      data: {
        path: 'src/big.ts',
        content: 'chunk-1',
        pagination: {
          currentPage: 1,
          totalPages: 2,
          hasMore: true,
          charOffset: 0,
          charLength: 2000,
          totalChars: 4000,
          nextCharOffset: 2000,
        },
      },
    };

    const out = run([query], [result]);
    const file = (
      out.structuredContent.results as Array<{ data?: { files?: unknown[] } }>
    )[0]?.data?.files?.[0] as {
      next?: { continueChars?: unknown; cloneForSemantics?: unknown };
    };

    expect(file.next?.continueChars).toBeDefined();
    expect(file.next?.cloneForSemantics).toBeUndefined();
  });

  it.each([
    'nonFile',
    'oversized',
    'binary',
    'fileLimit',
    'fetchFailed',
    'totalSizeLimit',
    'pathTraversal',
  ] as const)(
    'marks an incomplete directory caused by %s and emits a clone escalation',
    reason => {
      const skipped = {
        nonFile: 0,
        oversized: 0,
        binary: 0,
        fileLimit: 0,
        fetchFailed: 0,
        totalSizeLimit: 0,
        pathTraversal: 0,
        [reason]: 1,
      };
      const out = run(
        [
          {
            owner: 'octo',
            repo: 'engine',
            branch: 'main',
            path: 'src',
            type: 'directory',
          },
        ],
        [
          {
            index: 0,
            status: 'success',
            data: {
              path: 'src',
              localPath: '/tmp/engine/src',
              complete: false,
              skipped,
            },
          },
        ]
      );
      const row = (
        out.structuredContent.results as Array<{
          data?: { directories?: Array<Record<string, any>> };
          meta?: { diagnostics?: { partial?: boolean; codes?: string[] } };
        }>
      )[0]!;
      const directory = row.data?.directories?.[0]!;

      expect(directory).toMatchObject({
        complete: false,
        isPartial: true,
        partialReasons: [reason],
      });
      expect(directory.next.escalateToClone).toMatchObject({
        tool: 'ghCloneRepo',
        query: {
          owner: 'octo',
          repo: 'engine',
          branch: 'main',
          sparsePath: 'src',
        },
      });
    }
  );

  it('marks an incomplete directory terminal when clone is unavailable', () => {
    mockIsCloneEnabled.mockReturnValue(false);
    const out = run(
      [
        {
          owner: 'octo',
          repo: 'engine',
          path: 'src',
          type: 'directory',
        },
      ],
      [
        {
          index: 0,
          status: 'success',
          data: {
            path: 'src',
            localPath: '/tmp/engine/src',
            complete: false,
            skipped: { fetchFailed: 1 },
          },
        },
      ]
    );
    const row = (
      out.structuredContent.results as Array<{
        data?: { directories?: Array<Record<string, any>> };
        meta?: { diagnostics?: { codes?: string[] } };
      }>
    )[0]!;
    const directory = row.data?.directories?.[0]!;

    expect(directory).toMatchObject({
      complete: false,
      isPartial: true,
      terminalLimit: true,
      partialReasons: ['fetchFailed'],
    });
    expect(directory.next).toBeUndefined();
  });
});
