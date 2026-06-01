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

  it('uses the tool paginator before bulk response pagination leaves a single huge result oversized', async () => {
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

  it('bulk window slices BETWEEN repos, never inside one — topics[] stays whole (live-bug regression)', () => {
    // The live bug: a default responseCharLength windowed mid-repo and returned
    // a fragmented topics array like ["dx","f"]. Repos are atomic now, so any
    // repo that appears carries its complete topics[].
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
    // Not everything fit → the cursor reports more (paginated per whole repo).
    expect(response.responsePagination?.hasMore).toBe(true);
  });

  it('githubViewRepoStructure: a directory node is item-atomic — files[] never sliced mid-list', () => {
    // Unify structure onto the item-atomic model (like repos/packages): the
    // char backstop must emit a directory node whole or defer it — never a
    // partial files[] list. (The entry cursor already bounds page size; this is
    // the consistency guarantee for the rare overflow case.)
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
      { offset: 0, length: 60 }, // tight — the old paginateStructureEntry sliced files[]
      TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE
    );
    const structure = (
      response.results[0]?.data as {
        structure?: Record<string, { files?: string[] }>;
      }
    ).structure;
    const files = structure?.src?.files;
    // Atomic: the node is emitted whole (forward progress) with its FULL list,
    // never a truncated slice.
    if (files) expect(files).toEqual(allFiles);
  });

  it('clamps bulk currentPage when responseCharOffset is beyond the available content', () => {
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

  it('advances past a mid-segment resume into later queries instead of stalling', () => {
    // Multi-query repo bulk. A responseCharOffset that resumes MID query-1
    // must, when budget remains, pack the FOLLOWING queries too — not return
    // only query-1's tail and stall (the cursor-stall bug).
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
      // Offset well inside query-1's segment; ample length to span all three.
      { offset: 120, length: 100_000 },
      TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES
    );

    const ids = (response.results as Array<{ id: string }>).map(r => r.id);
    // The page must reach query-3, proving the cursor advanced past the
    // mid-segment resume rather than stopping at query-1's tail.
    expect(ids).toContain('repo_q3');
    expect(ids.length).toBeGreaterThan(1);
    expect(response.responsePagination?.hasMore).toBe(false);
  });

  it('paginates githubSearchRepositories at the WHOLE-REPO level — topics never sliced', () => {
    // Repos are atomic: char windowing slices BETWEEN repos, never inside one.
    // Two repos with full topics[]; a small budget pages them one at a time,
    // and each repo on the page keeps its complete topics array.
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

    // First repo present with its FULL topics array (no mid-array truncation).
    expect(data.repositories?.[0]?.repo).toBe('one');
    expect(data.repositories?.[0]?.topics).toEqual([
      'alpha',
      'beta',
      'gamma',
      'delta',
    ]);
    // The second repo didn't fit → paginated to the next page (per-item).
    expect(data.outputPagination?.hasMore).toBe(true);
  });

  it('paginates githubViewRepoStructure at the directory-NODE level — node files[] stay whole', () => {
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
      { charLength: 90 }, // tight → not all nodes fit
      TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE
    );

    const data = result.data as {
      structure?: Record<string, { files?: string[] }>;
      outputPagination?: { hasMore: boolean };
    };

    // Nodes paginate as whole units; each emitted node keeps its FULL files[].
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

  it('maps localFindFiles charPagination into outputPagination without re-slicing the file list', () => {
    const result = applyQueryOutputPagination(
      {
        id: 'find_files',
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

  it('paginates lspGotoDefinition locations through the location content branch', () => {
    const fullContent = 'definition-'.repeat(700);

    const result = applyQueryOutputPagination(
      {
        id: 'goto_def',
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
