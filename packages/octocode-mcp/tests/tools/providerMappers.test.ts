import { describe, expect, it } from 'vitest';
import {
  buildPaginationHints,
  mapCodeSearchProviderResult,
  mapCodeSearchToolQuery,
  mapPullRequestProviderResultData,
  mapPullRequestToolQuery,
  mapRepoSearchProviderRepositories,
  mapRepoStructureProviderResult,
} from '../../src/tools/providerMappers.js';

describe('providerMappers', () => {
  it('should map code search tool queries to provider queries', () => {
    expect(
      mapCodeSearchToolQuery({
        keywordsToSearch: ['needle'],
        owner: 'owner',
        repo: 'repo',
        path: 'src',
      })
    ).toEqual(
      expect.objectContaining({
        keywords: ['needle'],
        projectId: 'owner/repo',
        path: 'src',
      })
    );
  });

  it('should map code search provider results into tool output shape', () => {
    const result = mapCodeSearchProviderResult(
      {
        items: [
          {
            path: 'src/index.ts',
            matches: [{ context: 'const test = 1;', positions: [] }],
            url: '',
            repository: {
              id: '1',
              name: 'owner/repo',
              url: '',
            },
          },
        ],
        totalCount: 1,
        pagination: {
          currentPage: 1,
          totalPages: 1,
          hasMore: false,
          totalMatches: 1,
        },
      },
      {
        keywordsToSearch: ['test'],
      }
    );

    expect(result.results).toEqual([
      expect.objectContaining({
        id: 'owner/repo',
        owner: 'owner',
        repo: 'repo',
        matches: [{ path: 'src/index.ts', value: 'const test = 1;' }],
      }),
    ]);
  });

  it('should preserve subgroup owners when mapping code search results', () => {
    const result = mapCodeSearchProviderResult(
      {
        items: [
          {
            path: 'src/index.ts',
            matches: [{ context: 'const test = 1;', positions: [] }],
            url: '',
            repository: {
              id: '1',
              name: 'group/subgroup/repo',
              url: '',
            },
          },
        ],
        totalCount: 1,
        pagination: {
          currentPage: 1,
          totalPages: 1,
          hasMore: false,
          totalMatches: 1,
        },
      },
      {
        keywordsToSearch: ['test'],
      }
    );

    expect(result.results).toEqual([
      expect.objectContaining({
        id: 'group/subgroup/repo',
        owner: 'group/subgroup',
        repo: 'repo',
      }),
    ]);
  });

  it('should preserve subgroup owners when mapping repository results', () => {
    const result = mapRepoSearchProviderRepositories([
      {
        id: '1',
        name: 'repo',
        fullPath: 'group/subgroup/repo',
        description: 'test',
        url: 'https://example.com/group/subgroup/repo',
        cloneUrl: 'https://example.com/group/subgroup/repo.git',
        defaultBranch: 'main',
        stars: 0,
        forks: 0,
        visibility: 'public',
        topics: [],
        createdAt: '2024-01-01T00:00:00Z',
        updatedAt: '2024-01-01T00:00:00Z',
        lastActivityAt: '2024-01-01T00:00:00Z',
      },
    ]);

    expect(result[0]).toMatchObject({
      owner: 'group/subgroup',
      repo: 'repo',
    });
  });

  it('should preserve owner in code search query when repo is absent', () => {
    const result = mapCodeSearchToolQuery({
      keywordsToSearch: ['refund'],
      owner: 'wix-private',
    });

    expect(result.projectId).toBeUndefined();
    expect(result.owner).toBe('wix-private');
  });

  it('should preserve owner in PR search query when repo is absent', () => {
    const result = mapPullRequestToolQuery({
      owner: 'wix-private',
      state: 'open',
    });

    expect(result.projectId).toBeUndefined();
    expect(result.owner).toBe('wix-private');
  });

  it('should set both projectId and owner in code search when both are provided', () => {
    const result = mapCodeSearchToolQuery({
      keywordsToSearch: ['test'],
      owner: 'facebook',
      repo: 'react',
    });

    expect(result.projectId).toBe('facebook/react');
    expect(result.owner).toBe('facebook');
  });

  it('should set both projectId and owner in PR search when both are provided', () => {
    const result = mapPullRequestToolQuery({
      owner: 'facebook',
      repo: 'react',
      state: 'open',
    });

    expect(result.projectId).toBe('facebook/react');
    expect(result.owner).toBe('facebook');
  });

  it('should forward free-text query through PR mapper', () => {
    const result = mapPullRequestToolQuery({
      owner: 'facebook',
      repo: 'react',
      query: 'hydration',
      state: 'closed',
    });

    expect(result.query).toBe('hydration');
  });

  it('should preserve every provider PR response field in tool output', () => {
    const { resultData } = mapPullRequestProviderResultData({
      items: [
        {
          number: 123,
          title: 'Fix hydration',
          body: 'Detailed body',
          url: 'https://github.com/facebook/react/pull/123',
          state: 'merged',
          draft: false,
          author: 'alice',
          assignees: ['bob'],
          labels: ['bug'],
          sourceBranch: 'fix-hydration',
          targetBranch: 'main',
          sourceSha: 'abc123',
          targetSha: 'def456',
          createdAt: '2026-05-24T00:00:00Z',
          updatedAt: '2026-05-25T00:00:00Z',
          closedAt: '2026-05-25T01:00:00Z',
          mergedAt: '2026-05-25T01:00:00Z',
          commentsCount: 2,
          changedFilesCount: 1,
          additions: 10,
          deletions: 3,
          comments: [
            {
              id: 'c1',
              author: 'bob',
              body: 'Looks good',
              createdAt: '2026-05-25T00:30:00Z',
              updatedAt: '2026-05-25T00:30:00Z',
            },
          ],
          fileChanges: [
            {
              path: 'src/a.ts',
              status: 'modified',
              additions: 10,
              deletions: 3,
              patch: '@@ -1 +1 @@',
            },
          ],
        },
      ],
      totalCount: 1,
      pagination: {
        currentPage: 1,
        totalPages: 1,
        hasMore: false,
        totalMatches: 1,
      },
    });

    const [pr] = resultData.pull_requests as Array<Record<string, unknown>>;

    expect(Object.keys(pr!).sort()).toEqual(
      [
        'additions',
        'assignees',
        'author',
        'body',
        'changedFilesCount',
        'closedAt',
        'comments',
        'commentsCount',
        'createdAt',
        'deletions',
        'draft',
        'fileChanges',
        'labels',
        'mergedAt',
        'number',
        'reviewSummary',
        'sourceBranch',
        'sourceSha',
        'state',
        'targetBranch',
        'targetSha',
        'title',
        'updatedAt',
        'url',
      ].sort()
    );
    expect(pr!.sourceSha).toBe('abc123');
    expect(pr!.targetSha).toBe('def456');
    expect(pr!.reviewSummary).toMatchObject({
      totalComments: 1,
      commenters: ['bob'],
      themes: ['approval'],
    });
  });

  it('omits fileChanges entirely in metadata mode (includeFileChanges=false)', () => {
    const { resultData } = mapPullRequestProviderResultData(
      {
        items: [
          {
            number: 410,
            title: 'update docs',
            body: 'docs body',
            url: 'https://github.com/owner/repo/pull/410',
            state: 'merged',
            draft: false,
            author: 'alice',
            assignees: [],
            labels: [],
            sourceBranch: 'docs',
            targetBranch: 'main',
            sourceSha: 'aaa',
            targetSha: 'bbb',
            createdAt: '2026-05-24T00:00:00Z',
            updatedAt: '2026-05-25T00:00:00Z',
            additions: 90,
            deletions: 70,
            fileChanges: [
              {
                path: 'a.md',
                status: 'modified',
                additions: 50,
                deletions: 40,
                patch: '@@ -1 +1 @@\n-old\n+new',
              },
              {
                path: 'b.md',
                status: 'modified',
                additions: 40,
                deletions: 30,
              },
            ],
          },
        ],
        totalCount: 1,
        pagination: {
          currentPage: 1,
          totalPages: 1,
          hasMore: false,
          totalMatches: 1,
        },
      },
      { includeFileChanges: false }
    );

    const [pr] = resultData.pull_requests as Array<Record<string, unknown>>;
    expect(pr).not.toHaveProperty('fileChanges');
    expect(pr!.changedFilesCount).toBe(2);
    expect(pr!.additions).toBe(90);
    expect(pr!.deletions).toBe(70);
  });

  it('keeps the fileChanges array when includeFileChanges is true (default)', () => {
    const { resultData } = mapPullRequestProviderResultData({
      items: [
        {
          number: 411,
          title: 'feature',
          body: 'feat body',
          url: 'https://github.com/owner/repo/pull/411',
          state: 'open',
          draft: false,
          author: 'alice',
          assignees: [],
          labels: [],
          sourceBranch: 'feat',
          targetBranch: 'main',
          createdAt: '2026-05-24T00:00:00Z',
          updatedAt: '2026-05-25T00:00:00Z',
          fileChanges: [
            { path: 'a.ts', status: 'modified', additions: 1, deletions: 0 },
          ],
        },
      ],
      totalCount: 1,
      pagination: {
        currentPage: 1,
        totalPages: 1,
        hasMore: false,
        totalMatches: 1,
      },
    });

    const [pr] = resultData.pull_requests as Array<Record<string, unknown>>;
    expect(Array.isArray(pr!.fileChanges)).toBe(true);
    expect((pr!.fileChanges as unknown[]).length).toBe(1);
  });

  it('emits a single combined cursor line when hasMore', () => {
    const hints = buildPaginationHints(
      {
        currentPage: 2,
        totalPages: 3,
        hasMore: true,
        totalMatches: 25,
        perPage: 10,
      },
      'matches'
    );
    expect(hints).toHaveLength(1);
    expect(hints[0]).toContain('Page 2/3');
    expect(hints[0]).toContain('Next: page=3');
  });

  it('emits no hint on the final page (no tautology)', () => {
    expect(
      buildPaginationHints(
        {
          currentPage: 3,
          totalPages: 3,
          hasMore: false,
          totalMatches: 25,
          perPage: 10,
        },
        'matches'
      )
    ).toEqual([]);
  });

  it('should include branch fallback details for repo structure results', () => {
    const result = mapRepoStructureProviderResult(
      {
        projectPath: 'owner/repo',
        branch: 'main',
        defaultBranch: 'main',
        path: '',
        structure: {},
        summary: {
          totalFiles: 0,
          totalFolders: 0,
          truncated: false,
        },
      },
      {
        owner: 'owner',
        repo: 'repo',
        branch: 'feature-x',
      },
      {},
      'feature-x'
    );

    expect(result).toHaveProperty('branchFallback');
  });
});
