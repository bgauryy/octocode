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

describe('ghHistoryResearch shapePullRequestForContent — next hints', () => {
  it('emits a per-row next drill-down even for a lean list-mode row (regression: list mode used to dead-end)', () => {
    // Mirrors the lean shape pullRequestsMode builds for a plain list query
    // with no explicit content selection.
    const leanRequest = normalizePullRequestContentRequest({} as never);
    const shaped = shapePullRequestForContent(
      PR,
      { owner: 'octo', repo: 'engine' },
      leanRequest,
      false,
      true // pullRequestsMode now always passes showContentMap:true
    );

    expect(shaped.next).toBeDefined();
    const next = shaped.next as Record<string, { target?: { prNumber?: number } }>;
    expect(next.target).toBeDefined();
    expect((shaped.next as { target: { prNumber: number } }).target.prNumber).toBe(
      42
    );
  });

  it('omits next when showContentMap is explicitly false', () => {
    const leanRequest = normalizePullRequestContentRequest({} as never);
    const shaped = shapePullRequestForContent(
      PR,
      { owner: 'octo', repo: 'engine' },
      leanRequest,
      false,
      false
    );
    expect(shaped.next).toBeUndefined();
  });
});
