import {
  afterAll,
  beforeAll,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from 'vitest';

const recordedProvider = vi.hoisted(() => ({
  type: 'github' as const,
  capabilities: {
    cloneRepo: true,
    fetchDirectoryToDisk: true,
    requiresScopedCodeSearch: false,
    supportsMergedState: true,
    supportsMultiTopicSearch: true,
  },
  searchCode: vi.fn(),
  getFileContent: vi.fn(),
  searchRepos: vi.fn(),
  searchPullRequests: vi.fn(),
  getRepoStructure: vi.fn(),
  resolveDefaultBranch: vi.fn(),
}));

vi.mock('../../src/providers/factory.js', () => ({
  getProvider: () => recordedProvider,
}));

import { prepareDirectToolInput } from '../../src/tools/directToolCatalog/toolInputPreparation.js';
import { cleanup, initialize } from '../../src/serverConfig.js';
import { fetchMultipleGitHubFileContents } from '../../src/tools/github_fetch_content/execution.js';
import { executeGitHubSearch } from '../../src/tools/github_search/execution.js';
import { searchMultipleGitHubCode } from '../../src/tools/github_search_code/execution.js';
import { searchMultipleGitHubHistory } from '../../src/tools/github_search_pull_requests/historyExecutions.js';
import { searchMultipleGitHubRepos } from '../../src/tools/github_search_repos/execution.js';
import { exploreMultipleRepositoryStructures } from '../../src/tools/github_view_repo_structure/execution.js';

const pagination = {
  currentPage: 1,
  totalPages: 1,
  hasMore: false,
  entriesPerPage: 20,
  totalMatches: 1,
};

function prepared(tool: string, query: Record<string, unknown>): any {
  return (
    prepareDirectToolInput(tool, query, {
      rejectUnknownFields: true,
    }) as { queries: unknown[] }
  ).queries[0];
}

function providerResponse(data: unknown) {
  return Promise.resolve({
    provider: 'github' as const,
    status: 200,
    rawResponseChars: JSON.stringify(data).length,
    data,
  });
}

function firstRow(
  label: string,
  result: { structuredContent?: unknown }
): Record<string, any> {
  const content = result.structuredContent as {
    results?: Record<string, any>[];
  };
  expect(content.results, `${label}: ${JSON.stringify(content)}`).toHaveLength(
    1
  );
  return content.results?.[0] ?? {};
}

describe('recorded authenticated GitHub response smokes', () => {
  beforeAll(async () => {
    await initialize();
  });

  afterAll(() => {
    cleanup();
  });

  beforeEach(() => {
    vi.clearAllMocks();
    recordedProvider.resolveDefaultBranch.mockResolvedValue('main');
  });

  it('runs provider-backed tools through mapping, finalization, and the shared envelope', async () => {
    recordedProvider.searchCode.mockImplementation(() =>
      providerResponse({
        items: [
          {
            path: 'src/index.ts',
            matches: [
              { context: 'export const needle = true;', positions: [[13, 19]] },
            ],
            url: 'https://github.com/recorded/fixture/blob/main/src/index.ts',
            repository: {
              id: 'recorded/fixture',
              name: 'recorded/fixture',
              url: 'https://github.com/recorded/fixture',
            },
          },
        ],
        totalCount: 1,
        pagination,
        repositoryContext: {
          owner: 'recorded',
          repo: 'fixture',
          branch: 'main',
        },
      })
    );
    recordedProvider.getFileContent.mockImplementation(() =>
      providerResponse({
        path: 'src/index.ts',
        content: 'export const needle = true;\n',
        encoding: 'utf-8',
        size: 28,
        totalLines: 1,
        sourceChars: 28,
        contentView: 'none',
        ref: 'main',
      })
    );
    recordedProvider.searchRepos.mockImplementation(() =>
      providerResponse({
        repositories: [
          {
            id: 'recorded/fixture',
            name: 'fixture',
            fullPath: 'recorded/fixture',
            description: 'Recorded provider fixture',
            url: 'https://github.com/recorded/fixture',
            cloneUrl: 'https://github.com/recorded/fixture.git',
            defaultBranch: 'main',
            stars: 10,
            forks: 1,
            visibility: 'public',
            topics: ['fixture'],
            createdAt: '2024-01-01T00:00:00Z',
            updatedAt: '2024-01-02T00:00:00Z',
            lastActivityAt: '2024-01-02T00:00:00Z',
          },
        ],
        totalCount: 1,
        pagination,
      })
    );
    recordedProvider.searchPullRequests.mockImplementation(() =>
      providerResponse({
        items: [
          {
            number: 7,
            title: 'Recorded change',
            body: 'Deterministic fixture',
            state: 'open',
            draft: false,
            author: 'octocode',
            assignees: [],
            labels: ['test'],
            sourceBranch: 'recording',
            targetBranch: 'main',
            createdAt: '2024-01-01T00:00:00Z',
            updatedAt: '2024-01-02T00:00:00Z',
          },
        ],
        totalCount: 1,
        pagination,
        repositoryContext: { owner: 'recorded', repo: 'fixture' },
      })
    );
    recordedProvider.getRepoStructure.mockImplementation(() =>
      providerResponse({
        projectPath: 'recorded/fixture',
        branch: 'main',
        defaultBranch: 'main',
        path: 'src',
        structure: { src: { files: ['index.ts'], folders: [] } },
        summary: { totalFiles: 1, totalFolders: 0, truncated: false },
        pagination,
      })
    );

    const unified = await executeGitHubSearch({
      queries: [
        prepared('ghSearch', {
          operation: 'code',
          owner: 'recorded',
          repo: 'fixture',
          keywords: ['needle'],
        }),
        prepared('ghSearch', {
          operation: 'repositories',
          keywords: ['fixture'],
        }),
        prepared('ghSearch', {
          operation: 'tree',
          owner: 'recorded',
          repo: 'fixture',
          path: 'src',
          branch: 'main',
        }),
      ],
    });
    const unifiedRows = (
      unified.structuredContent as { results?: Record<string, any>[] }
    ).results;
    expect(unifiedRows).toHaveLength(3);
    expect(unifiedRows?.map(row => row.data.operation)).toEqual([
      'code',
      'repositories',
      'tree',
    ]);
    expect(unifiedRows?.map(row => row.index)).toEqual([0, 1, 2]);
    expect(unifiedRows?.every(row => row.status !== 'error')).toBe(true);

    const runs = await Promise.all([
      searchMultipleGitHubCode({
        queries: [
          {
            owner: 'recorded',
            repo: 'fixture',
            keywords: ['needle'],
          },
        ],
      }),
      fetchMultipleGitHubFileContents({
        queries: [
          prepared('ghGetFileContent', {
            owner: 'recorded',
            repo: 'fixture',
            path: 'src/index.ts',
            branch: 'main',
            matchString: 'needle',
          }),
        ],
      }),
      searchMultipleGitHubRepos({
        queries: [{ keywords: ['fixture'] }],
      }),
      searchMultipleGitHubHistory({
        queries: [
          prepared('ghSearchHistory', {
            operation: 'pullRequests',
            owner: 'recorded',
            repo: 'fixture',
            keywords: ['change'],
          }),
        ],
      }),
      exploreMultipleRepositoryStructures({
        queries: [
          {
            owner: 'recorded',
            repo: 'fixture',
            path: 'src',
            branch: 'main',
          },
        ],
      }),
    ]);

    const labels = ['code', 'content', 'repos', 'pullRequests', 'structure'];
    for (const [index, run] of runs.entries()) {
      const row = firstRow(labels[index] ?? String(index), run);
      expect(row.status).not.toBe('error');
      expect(row.meta).toEqual(
        expect.objectContaining({
          evidence: { kind: 'provider', confidence: 'medium' },
        })
      );
      expect(JSON.stringify(row)).not.toMatch(/authorization|access_token/i);
    }
  });
});
