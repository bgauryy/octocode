import { describe, expect, it } from 'vitest';

import { mapPullRequestProviderResultData } from '../../../src/tools/providerMappers/pullRequests.js';

function basePr(overrides: Record<string, unknown> = {}) {
  return {
    number: 1,
    title: 'Fix the thing',
    url: 'https://github.com/o/r/pull/1',
    state: 'open' as const,
    author: 'someone',
    createdAt: '2026-01-01T00:00:00Z',
    ...overrides,
  };
}

describe('ghHistoryResearch reviewSummary.themes — ground-truth review states', () => {
  it('a single clean APPROVED review is never mislabeled by an unrelated bot comment (regression)', () => {
    const result = mapPullRequestProviderResultData({
      items: [
        basePr({
          reviews: [
            {
              id: '1',
              user: 'eps1lon',
              state: 'APPROVED',
              body: 'Looks great, thanks!',
            },
          ],
          comments: [
            {
              author: 'size-limit[bot]',
              body: 'This PR **changes** 500 lines. Is that expected?',
              createdAt: '2026-01-02T00:00:00Z',
              updatedAt: '2026-01-02T00:00:00Z',
            },
          ],
        }),
      ],
    } as never);

    const pr = result.pullRequests[0] as {
      reviewSummary?: { themes: string[] };
    };
    expect(pr.reviewSummary?.themes).toEqual(['approval']);
    expect(pr.reviewSummary?.themes).not.toContain('changes-requested');
    expect(pr.reviewSummary?.themes).not.toContain('question');
  });

  it('a real CHANGES_REQUESTED review is reported regardless of comment wording', () => {
    const result = mapPullRequestProviderResultData({
      items: [
        basePr({
          reviews: [
            { id: '1', user: 'reviewer', state: 'CHANGES_REQUESTED', body: '' },
          ],
          comments: [
            {
              author: 'reviewer',
              body: 'please update the docs',
              createdAt: '2026-01-02T00:00:00Z',
              updatedAt: '2026-01-02T00:00:00Z',
            },
          ],
        }),
      ],
    } as never);

    const pr = result.pullRequests[0] as {
      reviewSummary?: { themes: string[] };
    };
    expect(pr.reviewSummary?.themes).toContain('changes-requested');
  });

  it('a genuine human question with no reviews falls back to comment-body heuristics', () => {
    const result = mapPullRequestProviderResultData({
      items: [
        basePr({
          comments: [
            {
              author: 'someone',
              body: 'why did this change?',
              createdAt: '2026-01-02T00:00:00Z',
              updatedAt: '2026-01-02T00:00:00Z',
            },
          ],
        }),
      ],
    } as never);

    const pr = result.pullRequests[0] as {
      reviewSummary?: { themes: string[] };
    };
    expect(pr.reviewSummary?.themes).toContain('question');
  });

  it('no reviews and no matching comment content falls back to "discussion"', () => {
    const result = mapPullRequestProviderResultData({
      items: [
        basePr({
          comments: [
            {
              author: 'someone',
              body: 'thanks for the update',
              createdAt: '2026-01-02T00:00:00Z',
              updatedAt: '2026-01-02T00:00:00Z',
            },
          ],
        }),
      ],
    } as never);

    const pr = result.pullRequests[0] as {
      reviewSummary?: { themes: string[] };
    };
    expect(pr.reviewSummary?.themes).toEqual(['discussion']);
  });
});
