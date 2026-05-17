import { beforeAll, describe, expect, it, vi } from 'vitest';
import {
  applyQueryOutputPagination,
  applyBulkResponsePagination,
} from '../../src/utils/response/structuredPagination.js';
import { executeBulkOperation } from '../../src/utils/response/bulk.js';
import { TOOL_NAMES } from '../../src/tools/toolMetadata/proxies.js';
import { initializeToolMetadata } from '../../src/tools/toolMetadata/state.js';

beforeAll(async () => {
  await initializeToolMetadata();
});

describe('tool-owned structured pagination', () => {
  it('paginates oversized githubSearchCode file matches instead of returning the full file payload', () => {
    const fullMatch = 'x'.repeat(12000);

    const result = applyQueryOutputPagination(
      {
        id: 'q1',
        status: 'hasResults',
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

  it('paginates githubCloneRepo hints when they are the dominant payload', () => {
    const hints = Array.from(
      { length: 20 },
      (_, index) => `Hint ${index}: ${'x'.repeat(250)}`
    );

    const result = applyQueryOutputPagination(
      {
        id: 'clone_repo',
        status: 'hasResults',
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

  it('keeps packageSearch pagination deterministic across pages', () => {
    const packages = Array.from({ length: 8 }, (_, index) => ({
      name: `pkg-${index}`,
      description: `package ${index} ${'x'.repeat(150)}`,
      repository: `https://github.com/octo/pkg-${index}`,
      version: '1.0.0',
    }));

    const firstPage = applyQueryOutputPagination(
      {
        id: 'pkg_search',
        status: 'hasResults',
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
        status: 'hasResults',
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

  it('uses the tool paginator before bulk response pagination leaves a single huge result oversized', async () => {
    const largeMatch = 'y'.repeat(5000);
    const processor = vi.fn().mockResolvedValue({
      status: 'hasResults' as const,
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
        responseCharLength: 1000,
      }
    );

    const structured = result.structuredContent as {
      results: Array<{
        data: {
          files?: Array<{ text_matches?: string[] }>;
          outputPagination?: { hasMore: boolean; charLength: number };
        };
      }>;
      responsePagination?: { hasMore: boolean; charLength: number };
    };

    expect(structured.responsePagination?.hasMore).toBe(true);
    expect(
      structured.results[0]?.data.files?.[0]?.text_matches?.[0]?.length
    ).toBeLessThan(largeMatch.length);
    expect(structured.results[0]?.data.outputPagination?.hasMore).toBe(true);
  });

  it('clamps bulk currentPage when responseCharOffset is beyond the available content', () => {
    const response = applyBulkResponsePagination(
      {
        results: [
          {
            id: 'repo_q1',
            status: 'hasResults',
            data: {
              repositories: [
                {
                  owner: 'octo',
                  repo: 'repo',
                  stars: 1,
                  description: 'small repo',
                  url: 'https://github.com/octo/repo',
                  createdAt: '2025-01-01T00:00:00Z',
                  updatedAt: '2025-01-01T00:00:00Z',
                  pushedAt: '2025-01-01T00:00:00Z',
                },
              ],
            },
          },
        ],
      },
      { offset: 500, length: 50 },
      TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES
    );

    expect(response.responsePagination).toBeDefined();
    expect(response.responsePagination?.currentPage).toBe(
      response.responsePagination?.totalPages
    );
  });

  it('paginates githubSearchRepositories topics through the repository branch', () => {
    const oversizedTopic = 'topic-'.repeat(500);

    const result = applyQueryOutputPagination(
      {
        id: 'repo_search',
        status: 'hasResults',
        data: {
          repositories: [
            {
              owner: 'octo',
              repo: 'repo',
              description: 'repository description',
              url: 'https://github.com/octo/repo',
              createdAt: '2025-01-01T00:00:00Z',
              updatedAt: '2025-01-01T00:00:00Z',
              pushedAt: '2025-01-01T00:00:00Z',
              topics: [oversizedTopic],
            },
          ],
        },
      },
      { charLength: 500 },
      TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES
    );

    const data = result.data as {
      repositories?: Array<{ repo: string; topics?: string[] }>;
      outputPagination?: { hasMore: boolean };
    };

    expect(data.repositories?.[0]?.repo).toBe('repo');
    expect(data.repositories?.[0]?.topics?.[0]?.length).toBeLessThan(
      oversizedTopic.length
    );
    expect(data.outputPagination?.hasMore).toBe(true);
  });

  it('paginates githubViewRepoStructure entries through the structure branch', () => {
    const files = Array.from(
      { length: 30 },
      (_, index) => `src/file-${index}.ts`
    );

    const result = applyQueryOutputPagination(
      {
        id: 'view_repo',
        status: 'hasResults',
        data: {
          structure: {
            src: {
              files,
              folders: ['nested'],
            },
          },
        },
      },
      { charLength: 400 },
      TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE
    );

    const data = result.data as {
      structure?: Record<string, { files?: string[] }>;
      outputPagination?: { hasMore: boolean };
    };

    expect(data.structure?.src?.files?.length).toBeLessThan(files.length);
    expect(data.outputPagination?.hasMore).toBe(true);
  });

  it('paginates localSearchCode match content through the match branch', () => {
    const fullMatch = 'match-'.repeat(700);

    const result = applyQueryOutputPagination(
      {
        id: 'local_search',
        status: 'hasResults',
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
        status: 'hasResults',
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

  it('maps localFindFiles charPagination into outputPagination without re-slicing the file list', () => {
    const result = applyQueryOutputPagination(
      {
        id: 'find_files',
        status: 'hasResults',
        data: {
          files: ['src/a.ts', 'src/b.ts'],
          charPagination: {
            currentPage: 1,
            totalPages: 2,
            hasMore: true,
            charOffset: 0,
            charLength: 100,
            totalChars: 200,
          },
        },
      },
      {},
      TOOL_NAMES.LOCAL_FIND_FILES
    );

    const data = result.data as {
      files?: string[];
      charPagination?: { charOffset?: number; charLength?: number };
      outputPagination?: { charOffset?: number; charLength?: number };
    };

    expect(data.files).toEqual(['src/a.ts', 'src/b.ts']);
    expect(data.outputPagination).toEqual(data.charPagination);
  });

  it('preserves githubGetFileContent content pagination instead of adding output pagination', () => {
    const result = applyQueryOutputPagination(
      {
        id: 'fetch_remote',
        status: 'hasResults',
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
        status: 'hasResults',
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

  it('paginates lspGotoDefinition locations through the location content branch', () => {
    const fullContent = 'definition-'.repeat(700);

    const result = applyQueryOutputPagination(
      {
        id: 'goto_def',
        status: 'hasResults',
        data: {
          locations: [
            {
              path: 'src/definition.ts',
              line: 15,
              content: fullContent,
            },
          ],
        },
      },
      { charLength: 500 },
      TOOL_NAMES.LSP_GOTO_DEFINITION
    );

    const data = result.data as {
      locations?: Array<{ content?: string }>;
      outputPagination?: { hasMore: boolean };
    };

    expect(data.locations?.[0]?.content?.length).toBeLessThan(
      fullContent.length
    );
    expect(data.outputPagination?.hasMore).toBe(true);
  });

  it('leaves lspFindReferences query data to its domain pagination contract', () => {
    const fullContent = 'reference-'.repeat(700);

    const result = applyQueryOutputPagination(
      {
        id: 'find_refs',
        status: 'hasResults',
        data: {
          locations: [
            {
              path: 'src/reference.ts',
              line: 22,
              content: fullContent,
            },
          ],
        },
      },
      { charLength: 500 },
      TOOL_NAMES.LSP_FIND_REFERENCES
    );

    const data = result.data as {
      locations?: Array<{ content?: string }>;
      outputPagination?: { hasMore: boolean };
    };

    expect(data.locations?.[0]?.content?.length).toBe(fullContent.length);
    expect(data.outputPagination).toBeUndefined();
  });

  it('uses only responsePagination for oversized lspFindReferences bulk responses', () => {
    const fullContent = 'reference-'.repeat(700);

    const response = applyBulkResponsePagination(
      {
        results: [
          {
            id: 'find_refs',
            status: 'hasResults',
            data: {
              locations: [
                {
                  uri: '/workspace/src/reference.ts',
                  range: { start: { line: 21, character: 0 } },
                  content: fullContent,
                },
              ],
              pagination: {
                currentPage: 1,
                totalPages: 2,
                totalResults: 2,
                hasMore: true,
                resultsPerPage: 1,
              },
            },
          },
        ],
      },
      { length: 500 },
      TOOL_NAMES.LSP_FIND_REFERENCES
    );

    const data = response.results[0]?.data as {
      outputPagination?: unknown;
      pagination?: unknown;
    };
    expect(response.responsePagination?.hasMore).toBe(true);
    expect(data.outputPagination).toBeUndefined();
    expect(data.pagination).toBeDefined();
  });

  it('paginates lspCallHierarchy call arrays through the hierarchy branch', () => {
    const incomingCalls = Array.from({ length: 30 }, (_, index) => ({
      from: `caller-${index}`,
      filePath: `/workspace/src/caller-${index}.ts`,
      line: index + 1,
    }));

    const result = applyQueryOutputPagination(
      {
        id: 'call_hierarchy',
        status: 'hasResults',
        data: {
          incomingCalls,
          outgoingCalls: [],
        },
      },
      { charLength: 500 },
      TOOL_NAMES.LSP_CALL_HIERARCHY
    );

    const data = result.data as {
      incomingCalls?: Array<{ from: string }>;
      outputPagination?: { hasMore: boolean };
    };

    expect(data.incomingCalls?.length).toBeLessThan(incomingCalls.length);
    expect(data.outputPagination?.hasMore).toBe(true);
  });

  it('paginates githubSearchPullRequests through the pull_requests branch', () => {
    const pullRequests = Array.from({ length: 25 }, (_, index) => ({
      number: index + 1,
      title: `Pull request ${index} ${'x'.repeat(80)}`,
      state: 'open',
      url: `https://github.com/octo/repo/pull/${index + 1}`,
    }));

    const result = applyQueryOutputPagination(
      {
        id: 'search_prs',
        status: 'hasResults',
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
      TOOL_NAMES.LSP_CALL_HIERARCHY
    );

    const data = result.data as Record<string, unknown>;
    expect(data.outputPagination).toBeUndefined();
    expect(data.charPagination).toBeUndefined();
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
      TOOL_NAMES.LSP_CALL_HIERARCHY
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
        status: 'hasResults',
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
      TOOL_NAMES.LSP_CALL_HIERARCHY
    );

    const data = result.data as Record<string, unknown>;
    expect(data.outputPagination).toBeDefined();
  });
});
