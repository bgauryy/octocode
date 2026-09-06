import { describe, expect, it } from 'vitest';

import { shapePullRequestForContent } from '../../../src/tools/github_search_pull_requests/contentResponse.js';
import { normalizePullRequestContentRequest } from '../../../src/tools/github_search_pull_requests/contentRequest.js';
import { GitHubGetHistoryItemQueryLocalSchema } from '../../../src/tools/github_search_pull_requests/historySchemes.js';

const PR = {
  number: 42,
  title: 'Fix the thing',
  state: 'open',
  author: 'someone',
  targetBranch: 'main',
  createdAt: '2026-01-01T00:00:00Z',
  mergedAt: null,
};

describe('pull-request exact-item nextCalls — copy-paste-ready fragments', () => {
  it('every fragment carries operation/owner/repo/number merged in', () => {
    const request = normalizePullRequestContentRequest({} as never);
    const shaped = shapePullRequestForContent(
      PR,
      { owner: 'octo', repo: 'engine' },
      request,
      false,
      true
    ) as {
      next: Record<string, { tool: string; query: Record<string, unknown> }>;
    };

    for (const call of Object.values(shaped.next)) {
      expect(call.tool).toBe('ghGetHistoryItem');
      expect(
        GitHubGetHistoryItemQueryLocalSchema.safeParse(call.query).success
      ).toBe(true);
    }
    expect(shaped.next.getBody?.query).toMatchObject({
      operation: 'pullRequest',
      owner: 'octo',
      repo: 'engine',
      number: 42,
      content: { body: true },
    });
    expect(shaped.next.getChangedFiles?.query).toMatchObject({
      owner: 'octo',
      repo: 'engine',
      operation: 'pullRequest',
      number: 42,
    });
    expect(shaped.next.fullReview?.query).toMatchObject({
      owner: 'octo',
      repo: 'engine',
      operation: 'pullRequest',
      number: 42,
      content: {
        body: true,
        changedFiles: true,
        patches: { mode: 'all' },
        comments: { discussion: true, reviewInline: true },
        reviews: true,
        commits: {},
      },
    });
  });

  it('getSelectedPatches uses a real changed-file path when changedFiles was already fetched this round', () => {
    const request = normalizePullRequestContentRequest({
      content: { changedFiles: true },
    } as never);
    const prWithFiles = {
      ...PR,
      fileChanges: [{ path: 'src/real-file.ts', status: 'modified' }],
    };
    const shaped = shapePullRequestForContent(
      prWithFiles,
      { owner: 'octo', repo: 'engine' },
      request,
      false,
      true
    ) as {
      next: {
        getSelectedPatches?: {
          query: { content: { patches: { files: string[] } } };
        };
      };
    };

    expect(shaped.next.getSelectedPatches?.query.content.patches.files).toEqual(
      ['src/real-file.ts']
    );
  });

  it('offers changed-file discovery without inventing a selected patch path', () => {
    const request = normalizePullRequestContentRequest({} as never);
    const shaped = shapePullRequestForContent(
      PR,
      { owner: 'octo', repo: 'engine' },
      request,
      false,
      true
    ) as {
      next: {
        getSelectedPatches?: { content: { patches: { files: string[] } } };
      };
    };

    expect(shaped.next.getSelectedPatches).toBeUndefined();
    expect(shaped.next).toHaveProperty('getChangedFiles');
    expect(JSON.stringify(shaped)).not.toContain('path/from/changedFiles');
  });
});
