import { describe, it, expect } from 'vitest';
import { transformRepoSearchResult } from '../../../../octocode-tools-core/src/providers/github/githubSearch.js';
import type { GitHubSearchRepositoriesData } from '@octocodeai/octocode-core/extra-types';

describe('transformRepoSearchResult — pagination page size', () => {
  it('carries the real page size into entriesPerPage (not a hardcoded 10)', () => {
    const data = {
      repositories: [
        {
          owner: 'facebook',
          repo: 'react',
          stars: 1,
          description: 'x',
          url: 'https://github.com/facebook/react',
          defaultBranch: 'main',
        },
      ],
      pagination: {
        currentPage: 1,
        totalPages: 5,
        perPage: 50,
        totalMatches: 213,
        hasMore: true,
      },
    } as unknown as GitHubSearchRepositoriesData;

    const result = transformRepoSearchResult(data);

    expect(result.pagination.entriesPerPage).toBe(50);
    expect(result.pagination.totalMatches).toBe(213);
    expect(result.pagination.hasMore).toBe(true);
  });

  it('propagates nonExistentScope through the transform', () => {
    const data = {
      repositories: [],
      nonExistentScope: true,
      pagination: {
        currentPage: 1,
        totalPages: 0,
        perPage: 30,
        totalMatches: 0,
        hasMore: false,
      },
    } as unknown as GitHubSearchRepositoriesData;

    const result = transformRepoSearchResult(data);
    expect(result.nonExistentScope).toBe(true);
  });
});
