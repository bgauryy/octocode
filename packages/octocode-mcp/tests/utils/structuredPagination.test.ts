import { beforeAll, describe, expect, it, vi } from 'vitest';
import {
  applyQueryOutputPagination,
  applyBulkResponsePagination,
} from '../../../octocode-tools-core/src/utils/response/structuredPagination.js';
import { executeBulkOperation } from '../../../octocode-tools-core/src/utils/response/bulk.js';
import { TOOL_NAMES } from '../../../octocode-tools-core/src/tools/toolMetadata/proxies.js';
import { LSP_GET_SEMANTIC_CONTENT_TOOL_NAME } from '../../../octocode-tools-core/src/tools/lsp/shared/semanticTypes.js';

beforeAll(async () => {});

describe('tool-owned structured pagination', () => {
  it('paginates oversized ghSearchCode file matches instead of returning the full file payload', () => {
    const fullMatch = 'x'.repeat(12000);

    const result = applyQueryOutputPagination(
      {
        id: 'q1',
        data: {
          files: [
            {
              path: 'src/big-file.ts',
              owner: 'octo',
              repo: 'repo',
              text_matches: [fullMatch],
            },
          ],
          hints: ['original hint'],
        },
      },
      { charLength: 1000 },
      TOOL_NAMES.GITHUB_SEARCH_CODE
    );

    const data = result.data as {
      files?: Array<{ path: string; text_matches?: string[] }>;
      outputPagination?: { hasMore: boolean; charLength: number };
      hints?: string[];
    };

    expect(data.files?.[0]?.path).toBe('src/big-file.ts');
    expect(data.files?.[0]?.text_matches?.[0]?.length).toBeLessThan(
      fullMatch.length
    );
    expect(data.outputPagination?.hasMore).toBe(true);
    expect(data.outputPagination?.charLength).toBeLessThanOrEqual(1300);
    expect(data.hints?.some(hint => hint.includes('Use charOffset='))).toBe(
      true
    );
  });

  it('paginates ghCloneRepo hints when they are the dominant payload', () => {
    const hints = Array.from(
      { length: 20 },
      (_, index) => `Hint ${index}: ${'x'.repeat(250)}`
    );

    const result = applyQueryOutputPagination(
      {
        id: 'clone_repo',
        data: {
          localPath: '/tmp/octocode/repo',
          hints,
        },
      },
      { charLength: 800 },
      TOOL_NAMES.GITHUB_CLONE_REPO
    );

    const data = result.data as {
      localPath: string;
      hints?: string[];
      outputPagination?: { hasMore: boolean; charLength: number };
    };

    expect(data.localPath).toBe('/tmp/octocode/repo');
    expect(data.hints?.length).toBeLessThan(hints.length);
    expect(data.outputPagination?.hasMore).toBe(true);
    expect(data.outputPagination?.charLength).toBeLessThanOrEqual(900);
  });

  it('keeps npmSearch pagination deterministic across pages', () => {
    const packages = Array.from({ length: 8 }, (_, index) => ({
      name: `pkg-${index}`,
      description: `package ${index} ${'x'.repeat(150)}`,
      repository: `https://github.com/octo/pkg-${index}`,
      version: '1.0.0',
    }));

    const firstPage = applyQueryOutputPagination(
      {
        id: 'pkg_search',
        data: {
          packages,
          totalFound: packages.length,
          hints: ['install hint'],
        },
      },
      { charLength: 1000 },
      TOOL_NAMES.PACKAGE_SEARCH
    );

    const firstData = firstPage.data as {
      packages?: Array<{ name: string }>;
      outputPagination?: {
        charOffset: number;
        charLength: number;
        hasMore: boolean;
      };
    };

    const nextOffset =
      (firstData.outputPagination?.charOffset ?? 0) +
      (firstData.outputPagination?.charLength ?? 0);

    const secondPage = applyQueryOutputPagination(
      {
        id: 'pkg_search',
        data: {
          packages,
          totalFound: packages.length,
          hints: ['install hint'],
        },
      },
      { charOffset: nextOffset, charLength: 1000 },
      TOOL_NAMES.PACKAGE_SEARCH
    );

    const secondData = secondPage.data as {
      packages?: Array<{ name: string }>;
    };

    expect(firstData.packages?.length).toBeGreaterThan(0);
    expect(firstData.outputPagination?.hasMore).toBe(true);
    expect(secondData.packages?.[0]?.name).not.toBe(
      firstData.packages?.[0]?.name
    );
  });

  it('uses the tool paginator (per-query outputPagination) for oversized results', async () => {
    const largeMatch = 'y'.repeat(5000);
    const processor = vi.fn().mockResolvedValue({
      files: [
        {
          path: 'src/large.ts',
          owner: 'octo',
          repo: 'repo',
          text_matches: [largeMatch],
        },
      ],
    });

    const result = await executeBulkOperation(
      [{ id: 'search_q1' }],
      processor,
      {
        toolName: TOOL_NAMES.GITHUB_SEARCH_CODE,
      }
    );

    const structured = result.structuredContent as {
      results: Array<{
        data: {
          files?: Array<{ text_matches?: string[] }>;
          outputPagination?: { hasMore: boolean };
        };
      }>;
    };

    expect(structured.results).toHaveLength(1);
  });

  it('bulk window slices BETWEEN repos, never inside one — topics[] stays whole (live-bug regression)', () => {
    const topics = ['alpha', 'beta', 'gamma', 'delta', 'epsilon'];
    const mkRepo = (n: string) => ({
      owner: 'octo',
      repo: n,
      stars: 1,
      description: 'a repository',
      url: `https://github.com/octo/${n}`,
      topics,
    });
    const response = applyBulkResponsePagination(
      {
        results: [
          {
            id: 'q1',
            data: {
              repositories: [
                mkRepo('one'),
                mkRepo('two'),
                mkRepo('three'),
                mkRepo('four'),
              ],
            },
          },
        ],
      },
      { offset: 0, length: 180 },
      TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES
    );
    const repos = (
      response.results[0]?.data as {
        repositories?: Array<{ topics?: string[] }>;
      }
    ).repositories;
    expect(repos?.length ?? 0).toBeGreaterThan(0);
    for (const r of repos ?? []) expect(r.topics).toEqual(topics);
  });

  it('ghViewRepoStructure: a directory node is item-atomic — files[] never sliced mid-list', () => {
    const allFiles = ['a.ts', 'b.ts', 'c.ts', 'd.ts', 'e.ts', 'f.ts'];
    const response = applyBulkResponsePagination(
      {
        results: [
          {
            id: 's',
            data: { structure: { src: { files: [...allFiles], folders: [] } } },
          },
        ],
      },
      { offset: 0, length: 60 },
      TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE
    );
    const structure = (
      response.results[0]?.data as {
        structure?: Record<string, { files?: string[] }>;
      }
    ).structure;
    const files = structure?.src?.files;
    if (files) expect(files).toEqual(allFiles);
  });

  it('returns empty results when offset is beyond the available content', () => {
    const response = applyBulkResponsePagination(
      {
        results: [
          {
            id: 'repo_q1',
            data: {
              repositories: [
                {
                  owner: 'octo',
                  repo: 'repo',
                  stars: 1,
                  description: 'small repo',
                  url: 'https://github.com/octo/repo',
                },
              ],
            },
          },
        ],
      },
      { offset: 500_000, length: 50 },
      TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES
    );

    expect(response.results).toHaveLength(0);
  });

  it('advances past a mid-segment resume into later queries instead of stalling', () => {
    const mkRepo = (owner: string, repo: string) => ({
      owner,
      repo,
      stars: 1,
      description: `description for ${owner}/${repo}`,
      url: `https://github.com/${owner}/${repo}`,
      createdAt: '2025-01-01T00:00:00Z',
      updatedAt: '2025-01-01T00:00:00Z',
      pushedAt: '2025-01-01T00:00:00Z',
    });
    const response = applyBulkResponsePagination(
      {
        results: [
          { id: 'repo_q1', data: { repositories: [mkRepo('a', 'one')] } },
          { id: 'repo_q2', data: { repositories: [mkRepo('b', 'two')] } },
          { id: 'repo_q3', data: { repositories: [mkRepo('c', 'three')] } },
        ],
      },
      { offset: 120, length: 100_000 },
      TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES
    );

    const ids = (response.results as Array<{ id: string }>).map(r => r.id);
    expect(ids).toContain('repo_q3');
    expect(ids.length).toBeGreaterThan(1);
  });

  it('paginates ghSearchRepos at the WHOLE-REPO level — topics never sliced', () => {
    const mkRepo = (n: string) => ({
      owner: 'octo',
      repo: n,
      description: 'repository description',
      url: `https://github.com/octo/${n}`,
      createdAt: '2025-01-01T00:00:00Z',
      updatedAt: '2025-01-01T00:00:00Z',
      pushedAt: '2025-01-01T00:00:00Z',
      topics: ['alpha', 'beta', 'gamma', 'delta'],
    });
    const result = applyQueryOutputPagination(
      {
        id: 'repo_search',
        data: { repositories: [mkRepo('one'), mkRepo('two')] },
      },
      { charLength: 200 },
      TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES
    );

    const data = result.data as {
      repositories?: Array<{ repo: string; topics?: string[] }>;
      outputPagination?: { hasMore: boolean };
    };

    expect(data.repositories?.[0]?.repo).toBe('one');
    expect(data.repositories?.[0]?.topics).toEqual([
      'alpha',
      'beta',
      'gamma',
      'delta',
    ]);
    expect(data.outputPagination?.hasMore).toBe(true);
  });

  it('paginates ghViewRepoStructure at the directory-NODE level — node files[] stay whole', () => {
    const mk = (p: string) =>
      ['1', '2', '3', '4'].map(n => `${p}/file-${n}.ts`);
    const result = applyQueryOutputPagination(
      {
        id: 'view_repo',
        data: {
          structure: {
            src: { files: mk('src'), folders: [] },
            lib: { files: mk('lib'), folders: [] },
            test: { files: mk('test'), folders: [] },
          },
        },
      },
      { charLength: 90 },
      TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE
    );

    const data = result.data as {
      structure?: Record<string, { files?: string[] }>;
      outputPagination?: { hasMore: boolean };
    };

    expect(data.outputPagination?.hasMore).toBe(true);
    const nodes = Object.values(data.structure ?? {});
    expect(nodes.length).toBeGreaterThan(0);
    for (const node of nodes) expect(node.files?.length).toBe(4);
  });

  it('paginates localSearchCode match content through the match branch', () => {
    const fullMatch = 'match-'.repeat(700);

    const result = applyQueryOutputPagination(
      {
        id: 'local_search',
        data: {
          files: [
            {
              path: 'src/search.ts',
              matches: [
                {
                  lineNumber: 10,
                  value: fullMatch,
                },
              ],
            },
          ],
        },
      },
      { charLength: 500 },
      TOOL_NAMES.LOCAL_RIPGREP
    );

    const data = result.data as {
      files?: Array<{ matches?: Array<{ value?: string }> }>;
      outputPagination?: { hasMore: boolean };
    };

    expect(data.files?.[0]?.matches?.[0]?.value?.length).toBeLessThan(
      fullMatch.length
    );
    expect(data.outputPagination?.hasMore).toBe(true);
  });

  it('paginates localViewStructure entries through the entries branch', () => {
    const entries = Array.from({ length: 40 }, (_, index) => ({
      name: `entry-${index}`,
      type: index % 2 === 0 ? 'file' : 'directory',
      path: `/workspace/entry-${index}`,
    }));

    const result = applyQueryOutputPagination(
      {
        id: 'view_local',
        data: {
          entries,
        },
      },
      { charLength: 450 },
      TOOL_NAMES.LOCAL_VIEW_STRUCTURE
    );

    const data = result.data as {
      entries?: Array<{ name: string }>;
      outputPagination?: { hasMore: boolean };
    };

    expect(data.entries?.length).toBeLessThan(entries.length);
    expect(data.outputPagination?.hasMore).toBe(true);
  });

  it('leaves localFindFiles data unchanged on non-explicit pagination path', () => {
    const queryResult = {
      id: 'find_files',
      data: {
        files: ['src/a.ts', 'src/b.ts'],
        pagination: { currentPage: 1, totalPages: 1, hasMore: false },
      },
    };

    const result = applyQueryOutputPagination(
      queryResult,
      {},
      TOOL_NAMES.LOCAL_FIND_FILES
    );

    expect(result).toBe(queryResult);
    const data = result.data as { files?: string[] };
    expect(data.files).toEqual(['src/a.ts', 'src/b.ts']);
  });

  it('preserves ghGetFileContent content pagination instead of adding output pagination', () => {
    const result = applyQueryOutputPagination(
      {
        id: 'fetch_remote',
        data: {
          path: 'src/file.ts',
          content: 'const x = 1;',
          pagination: {
            currentPage: 1,
            totalPages: 2,
            hasMore: true,
            charOffset: 0,
            charLength: 12,
            totalChars: 24,
          },
        },
      },
      { charLength: 5 },
      TOOL_NAMES.GITHUB_FETCH_CONTENT
    );

    const data = result.data as {
      content?: string;
      pagination?: { hasMore: boolean };
      outputPagination?: unknown;
    };

    expect(data.content).toBe('const x = 1;');
    expect(data.pagination?.hasMore).toBe(true);
    expect(data.outputPagination).toBeUndefined();
  });

  it('preserves localGetFileContent content pagination instead of adding output pagination', () => {
    const result = applyQueryOutputPagination(
      {
        id: 'fetch_local',
        data: {
          filePath: '/workspace/file.ts',
          content: 'export const x = 1;',
          pagination: {
            currentPage: 1,
            totalPages: 3,
            hasMore: true,
            charOffset: 0,
            charLength: 18,
            totalChars: 54,
          },
        },
      },
      { charLength: 5 },
      TOOL_NAMES.LOCAL_FETCH_CONTENT
    );

    const data = result.data as {
      content?: string;
      pagination?: { hasMore: boolean };
      outputPagination?: unknown;
    };

    expect(data.content).toBe('export const x = 1;');
    expect(data.pagination?.hasMore).toBe(true);
    expect(data.outputPagination).toBeUndefined();
  });

  it('paginates ghHistoryResearch through the pull_requests branch', () => {
    const pullRequests = Array.from({ length: 25 }, (_, index) => ({
      number: index + 1,
      title: `Pull request ${index} ${'x'.repeat(80)}`,
      state: 'open',
      url: `https://github.com/octo/repo/pull/${index + 1}`,
    }));

    const result = applyQueryOutputPagination(
      {
        id: 'search_prs',
        data: {
          pull_requests: pullRequests,
        },
      },
      { charLength: 500 },
      TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS
    );

    const data = result.data as {
      pull_requests?: Array<{ number: number }>;
      outputPagination?: { hasMore: boolean };
    };

    expect(data.pull_requests?.length).toBeLessThan(pullRequests.length);
    expect(data.outputPagination?.hasMore).toBe(true);
  });

  it('does NOT inject outputPagination into error-status query data (schema is strict)', () => {
    const longError = 'symbol not found: ' + 'x'.repeat(20000);

    const result = applyQueryOutputPagination(
      {
        id: 'q-err',
        status: 'error',
        data: { error: longError, hints: ['Retry with lineHint'] },
      },
      { charLength: 1000 },
      LSP_GET_SEMANTIC_CONTENT_TOOL_NAME
    );

    const data = result.data as Record<string, unknown>;
    expect(data.outputPagination).toBeUndefined();
    expect(data.error).toBe(longError);
  });

  it('does NOT inject outputPagination into empty-status query data', () => {
    const result = applyQueryOutputPagination(
      {
        id: 'q-empty',
        status: 'empty',
        data: { hints: ['No matches'] },
      },
      { charLength: 100 },
      LSP_GET_SEMANTIC_CONTENT_TOOL_NAME
    );

    const data = result.data as Record<string, unknown>;
    expect(data.outputPagination).toBeUndefined();
  });

  it('still injects outputPagination for hasResults-status oversized data', () => {
    const calls = Array.from({ length: 200 }, (_, i) => ({
      from: {
        name: `caller${i}`,
        kind: 'function' as const,
        uri: `file:///c${i}.ts`,
        range: {
          start: { line: i, character: 0 },
          end: { line: i, character: 10 },
        },
        content: 'x'.repeat(200),
      },
      fromRanges: [
        {
          start: { line: i, character: 0 },
          end: { line: i, character: 10 },
        },
      ],
    }));

    const result = applyQueryOutputPagination(
      {
        id: 'q-ok',
        data: {
          item: {
            name: 'target',
            kind: 'function',
            uri: 'file:///t.ts',
            range: {
              start: { line: 0, character: 0 },
              end: { line: 0, character: 10 },
            },
          },
          direction: 'incoming',
          depth: 1,
          incomingCalls: calls,
        },
      },
      { charLength: 1000 },
      LSP_GET_SEMANTIC_CONTENT_TOOL_NAME
    );

    const data = result.data as Record<string, unknown>;
    expect(data.outputPagination).toBeDefined();
  });
});
