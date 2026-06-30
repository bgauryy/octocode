/**
 * Regression / TDD tests for providerMapper bugs:
 *
 *  Bug #2 — capFileChanges is dead code (never truncates, wasTruncated never
 *            checked): refactor removes it; behavior must be preserved.
 *
 *  Bug #3 — splitRepositoryPath is defined twice in the same file (module-level
 *            and again inside mapRepoSearchProviderRepositories): dedup; tests
 *            must stay green before and after.
 */
import { describe, it, expect } from 'vitest';

import {
  mapPullRequestProviderResultData,
  mapRepoSearchProviderRepositories,
} from '../../src/tools/providerMappers.js';
import type { PullRequestSearchResult } from '../../src/providers/types.js';

// ---------------------------------------------------------------------------
// Bug #2 — capFileChanges dead-code removal
// ---------------------------------------------------------------------------
function makePR(
  overrides: Partial<PullRequestSearchResult['items'][number]> = {}
): PullRequestSearchResult['items'][number] {
  return {
    number: 42,
    title: 'Test PR',
    body: null,
    url: 'https://github.com/owner/repo/pull/42',
    state: 'open',
    draft: false,
    author: 'alice',
    assignees: [],
    labels: [],
    sourceBranch: 'feat/test',
    targetBranch: 'main',
    sourceSha: 'abc123',
    targetSha: 'def456',
    createdAt: '2024-01-01T00:00:00Z',
    updatedAt: '2024-01-02T00:00:00Z',
    commentsCount: 0,
    ...overrides,
  };
}

function makeData(
  prOverrides: Partial<PullRequestSearchResult['items'][number]> = {}
): PullRequestSearchResult {
  return {
    items: [makePR(prOverrides)],
    totalCount: 1,
    pagination: {
      currentPage: 1,
      totalPages: 1,
      entriesPerPage: 20,
      totalMatches: 1,
      hasMore: false,
    },
  };
}

describe('mapPullRequestProviderResultData – changedFilesCount fallback (Bug #2)', () => {
  it('uses changedFilesCount directly when the provider supplies it', () => {
    const data = makeData({ changedFilesCount: 7, fileChanges: undefined });
    const { pullRequests } = mapPullRequestProviderResultData(data);
    expect(pullRequests[0].changedFilesCount).toBe(7);
  });

  it('falls back to fileChanges.length when changedFilesCount is absent', () => {
    const data = makeData({
      changedFilesCount: undefined,
      fileChanges: [
        { path: 'a.ts', status: 'modified', additions: 5, deletions: 2 },
        { path: 'b.ts', status: 'added', additions: 10, deletions: 0 },
        { path: 'c.ts', status: 'removed', additions: 0, deletions: 3 },
      ],
    });
    const { pullRequests } = mapPullRequestProviderResultData(data);
    expect(pullRequests[0].changedFilesCount).toBe(3);
  });

  it('handles absent changedFilesCount with empty fileChanges gracefully', () => {
    const data = makeData({ changedFilesCount: undefined, fileChanges: [] });
    const { pullRequests } = mapPullRequestProviderResultData(data);
    // 0 files changed — count should be 0, not undefined
    expect(pullRequests[0].changedFilesCount).toBe(0);
  });

  it('prefers changedFilesCount over fileChanges.length when both are present', () => {
    // Provider-reported count is authoritative (may differ from patches included)
    const data = makeData({
      changedFilesCount: 12,
      fileChanges: [
        { path: 'a.ts', status: 'modified', additions: 1, deletions: 1 },
      ],
    });
    const { pullRequests } = mapPullRequestProviderResultData(data);
    expect(pullRequests[0].changedFilesCount).toBe(12);
  });
});

// ---------------------------------------------------------------------------
// Bug #3 — duplicate splitRepositoryPath
// ---------------------------------------------------------------------------
describe('mapRepoSearchProviderRepositories – owner/repo split (Bug #3)', () => {
  function makeRepo(
    fullPath: string
  ): Parameters<typeof mapRepoSearchProviderRepositories>[0][number] {
    return {
      id: fullPath,
      name: fullPath.split('/').pop() ?? fullPath,
      fullPath,
      description: null,
      url: `https://github.com/${fullPath}`,
      cloneUrl: `https://github.com/${fullPath}.git`,
      defaultBranch: 'main',
      stars: 0,
      forks: 0,
      visibility: 'public',
      topics: [],
      createdAt: '2024-01-01T00:00:00Z',
      updatedAt: '2024-01-01T00:00:00Z',
      lastActivityAt: '2024-01-01T00:00:00Z',
    };
  }

  it('splits a standard owner/repo fullPath', () => {
    const [result] = mapRepoSearchProviderRepositories([makeRepo('facebook/react')]);
    expect(result.owner).toBe('facebook');
    expect(result.repo).toBe('react');
  });

  it('handles a scoped org with slashes correctly (last slash is the separator)', () => {
    const [result] = mapRepoSearchProviderRepositories([
      makeRepo('my-org/sub/deep-repo'),
    ]);
    expect(result.repo).toBe('deep-repo');
    expect(result.owner).toBe('my-org/sub');
  });

  it('handles a bare repo name with no slash', () => {
    const [result] = mapRepoSearchProviderRepositories([makeRepo('bare-repo')]);
    expect(result.owner).toBe('');
    expect(result.repo).toBe('bare-repo');
  });

  it('maps all repositories in the list', () => {
    const repos = [
      makeRepo('vercel/next.js'),
      makeRepo('microsoft/typescript'),
    ];
    const results = mapRepoSearchProviderRepositories(repos);
    expect(results).toHaveLength(2);
    expect(results[0].owner).toBe('vercel');
    expect(results[1].owner).toBe('microsoft');
  });
});
