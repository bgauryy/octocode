import { describe, expect, it } from 'vitest';

import { shapePullRequestForContent } from '../../../src/tools/github_search_pull_requests/contentResponse.js';
import { normalizePullRequestContentRequest } from '../../../src/tools/github_search_pull_requests/contentRequest.js';

const PR = {
  number: 42,
  title: 'Fix the thing',
  state: 'open',
  author: 'someone',
  targetBranch: 'main',
  createdAt: '2026-01-01T00:00:00Z',
  mergedAt: null,
};

describe('ghHistoryResearch nextCalls — copy-paste-ready fragments (regression)', () => {
  it('every fragment carries owner/repo/prNumber merged in, not just `target`', () => {
    const request = normalizePullRequestContentRequest({} as never);
    const shaped = shapePullRequestForContent(
      PR,
      { owner: 'octo', repo: 'engine' },
      request,
      false,
      true
    ) as { next: Record<string, { owner?: string; prNumber?: number }> };

    expect(shaped.next.getBody).toMatchObject({
      owner: 'octo',
      repo: 'engine',
      prNumber: 42,
      content: { body: true },
    });
    expect(shaped.next.getChangedFiles).toMatchObject({
      owner: 'octo',
      repo: 'engine',
      prNumber: 42,
    });
    expect(shaped.next.fullReview).toMatchObject({
      owner: 'octo',
      repo: 'engine',
      prNumber: 42,
      reviewMode: 'full',
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
      next: { getSelectedPatches?: { content: { patches: { files: string[] } } } };
    };

    expect(shaped.next.getSelectedPatches?.content.patches.files).toEqual([
      'src/real-file.ts',
    ]);
  });

  it('falls back to the labeled placeholder when changedFiles was not fetched this round', () => {
    const request = normalizePullRequestContentRequest({} as never);
    const shaped = shapePullRequestForContent(
      PR,
      { owner: 'octo', repo: 'engine' },
      request,
      false,
      true
    ) as {
      next: { getSelectedPatches?: { content: { patches: { files: string[] } } } };
    };

    expect(shaped.next.getSelectedPatches?.content.patches.files).toEqual([
      'path/from/changedFiles',
    ]);
  });
});
