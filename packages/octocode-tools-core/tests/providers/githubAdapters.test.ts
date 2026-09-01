import { describe, expect, it } from 'vitest';

import {
  getFileContent,
  transformFileContentResult,
} from '../../src/providers/github/githubContent.js';
import {
  buildGitHubPullRequestsSearchParams,
  transformPullRequestResult,
} from '../../src/providers/github/githubPullRequests.js';
import {
  transformCodeSearchResult,
  transformRepoSearchResult,
} from '../../src/providers/github/githubSearch.js';
import {
  getRepoStructure,
  transformRepoStructureResult,
} from '../../src/providers/github/githubStructure.js';

describe('GitHub provider adapters', () => {
  it('maps file content and preserves actionable no-match warnings', () => {
    const result = transformFileContentResult(
      {
        path: 'README.md',
        content: '',
        branch: 'main',
        totalLines: 42,
        matchNotFound: true,
        searchedFor: 'missing-anchor',
      } as never,
      { projectId: 'owner/repo', path: 'README.md' }
    );

    expect(result).toMatchObject({
      path: 'README.md',
      ref: 'main',
      totalLines: 42,
      matchNotFound: true,
    });
    expect(result.warnings?.[0]).toContain('42 lines scanned');
    expect(result.warnings?.[0]).toContain('matchStringIsRegex=true');
  });

  it('maps code and repository search pagination without losing metadata', () => {
    const code = transformCodeSearchResult({
      items: [
        {
          path: 'src/index.ts',
          matches: [{ context: 'needle', positions: [[0, 6]] }],
          url: 'https://github.example/file',
          repository: {
            nameWithOwner: 'owner/repo',
            url: 'https://github.example/owner/repo',
          },
        },
      ],
      total_count: 1,
      pagination: {
        currentPage: 1,
        totalPages: 2,
        hasMore: true,
        totalMatches: 1,
        perPage: 1,
      },
    } as never);
    expect(code.pagination).toMatchObject({ hasMore: true, nextPage: 2 });
    expect(code.items[0]?.repository.id).toBe('owner/repo');

    const repos = transformRepoSearchResult({
      repositories: [
        {
          owner: 'owner',
          repo: 'repo',
          url: 'https://github.example/owner/repo',
          defaultBranch: 'trunk',
          stars: 3,
          forksCount: 2,
          visibility: 'public',
          topics: ['tools'],
        },
      ],
      pagination: { currentPage: 1, totalPages: 1, hasMore: false },
    } as never);
    expect(repos.repositories[0]).toMatchObject({
      id: 'owner/repo',
      defaultBranch: 'trunk',
      stars: 3,
      forks: 2,
    });
  });

  it('maps pull requests and converts merged state into GitHub search params', () => {
    const query = { projectId: 'owner/repo', state: 'merged' as const };
    const result = transformPullRequestResult(
      {
        pullRequests: [
          {
            number: 7,
            title: 'Ship it',
            state: 'closed',
            merged: true,
            author: 'octocat',
          },
        ],
        totalCount: 1,
      } as never,
      query
    );

    expect(result.items[0]).toMatchObject({ number: 7, state: 'merged' });
    expect(result.repositoryContext).toEqual({ owner: 'owner', repo: 'repo' });
    expect(
      buildGitHubPullRequestsSearchParams(query, 'owner', 'repo')
    ).toMatchObject({ owner: 'owner', repo: 'repo', state: 'closed', merged: true });
  });

  it('maps repository structure and rejects missing project scope before I/O', async () => {
    const transformed = transformRepoStructureResult({
      owner: 'owner',
      repo: 'repo',
      branch: 'main',
      path: '',
      structure: { '.': { files: ['README.md'], folders: [] } },
      summary: { totalFiles: 1, totalFolders: 0, truncated: false },
    } as never);
    expect(transformed).toMatchObject({
      projectPath: 'owner/repo',
      branch: 'main',
      path: '/',
      summary: { totalFiles: 1 },
    });

    await expect(
      getFileContent({ path: 'README.md' })
    ).resolves.toMatchObject({ status: 400, provider: 'github' });
    await expect(getRepoStructure({})).resolves.toMatchObject({
      status: 400,
      provider: 'github',
    });
  });
});
