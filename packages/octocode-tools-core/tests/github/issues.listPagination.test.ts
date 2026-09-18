import { describe, expect, it, vi, beforeEach } from 'vitest';

const mocks = vi.hoisted(() => ({
  getOctokit: vi.fn(),
  listForRepo: vi.fn(),
}));

vi.mock('../../src/github/client.js', () => ({
  getOctokit: mocks.getOctokit,
}));

const { listIssues } = await import('../../src/github/issues/fetchers.js');

function octokitWith(listResponse: unknown) {
  return {
    rest: {
      issues: {
        listForRepo: mocks.listForRepo.mockResolvedValue(listResponse),
      },
    },
  };
}

const PR_ROW = {
  number: 7,
  title: 'a pull request',
  state: 'open',
  user: { login: 'someone' },
  pull_request: { url: 'https://api.github.com/...' },
};

const ISSUE_ROW = {
  number: 8,
  title: 'a real issue',
  state: 'open',
  user: { login: 'someone' },
};

describe('listIssues pagination honesty', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('does not report totalCount:0 with hasMore:true — omits the misleading total and explains the PR-only page', async () => {
    mocks.getOctokit.mockResolvedValue(
      octokitWith({
        data: [PR_ROW, PR_ROW],
        headers: {
          link: '<https://api.github.com/repos/o/r/issues?page=2>; rel="next"',
        },
      })
    );

    const result = await listIssues({ owner: 'o', repo: 'r' });

    expect(result.data?.issues).toEqual([]);
    expect(result.data?.pagination?.hasMore).toBe(true);
    // A page-local post-filter count must not masquerade as a repo total
    // next to hasMore:true.
    expect(result.data?.totalCount).toBeUndefined();
    expect(
      result.data?.warnings?.some((w: string) => w.includes('pull requests'))
    ).toBe(true);
  });

  it('reports totalCount when the single page IS the complete result set', async () => {
    mocks.getOctokit.mockResolvedValue(
      octokitWith({
        data: [ISSUE_ROW],
        headers: {},
      })
    );

    const result = await listIssues({ owner: 'o', repo: 'r' });

    expect(result.data?.issues).toHaveLength(1);
    expect(result.data?.totalCount).toBe(1);
    expect(result.data?.pagination?.hasMore).toBe(false);
  });

  it('omits totalCount on any partial page (hasMore:true)', async () => {
    mocks.getOctokit.mockResolvedValue(
      octokitWith({
        data: [ISSUE_ROW],
        headers: {
          link: '<https://api.github.com/repos/o/r/issues?page=2>; rel="next"',
        },
      })
    );

    const result = await listIssues({ owner: 'o', repo: 'r' });

    expect(result.data?.totalCount).toBeUndefined();
    expect(result.data?.pagination?.hasMore).toBe(true);
  });

  it('skips a bounded run of PR-only pages and continues from the actual provider page', async () => {
    const pages = [
      ...Array.from({ length: 5 }, () => ({
        data: [PR_ROW],
        headers: {
          link: '<https://api.github.com/repos/o/r/issues?page=next>; rel="next"',
        },
      })),
      {
        data: [ISSUE_ROW],
        headers: {
          link: '<https://api.github.com/repos/o/r/issues?page=next>; rel="next"',
        },
      },
      { data: [{ ...ISSUE_ROW, number: 9 }], headers: {} },
    ];
    mocks.getOctokit.mockResolvedValue({
      rest: {
        issues: {
          listForRepo: mocks.listForRepo.mockImplementation(
            ({ page }: { page: number }) => pages[page - 1]
          ),
        },
      },
    });

    const result = await listIssues({ owner: 'o', repo: 'r', page: 1 });

    expect(mocks.listForRepo).toHaveBeenCalledTimes(6);
    expect(
      mocks.listForRepo.mock.calls.map(([request]) => request.page)
    ).toEqual([1, 2, 3, 4, 5, 6]);
    expect(result.data?.issues).toHaveLength(1);
    expect(result.data?.pagination).toMatchObject({
      currentPage: 6,
      nextPage: 7,
      requestedPage: 1,
      skippedPullRequestPages: 5,
      providerPagesFetched: 6,
      hasMore: true,
    });
    const next = await listIssues({ owner: 'o', repo: 'r', page: 7 });
    expect(
      [...(result.data?.issues ?? []), ...(next.data?.issues ?? [])].map(
        issue => (issue as { number: number }).number
      )
    ).toEqual([8, 9]);
  });

  it('stops at the useful-page skip budget and exposes the actual next provider page', async () => {
    mocks.getOctokit.mockResolvedValue(
      octokitWith({
        data: [PR_ROW],
        headers: {
          link: '<https://api.github.com/repos/o/r/issues?page=next>; rel="next"',
        },
      })
    );

    const result = await listIssues({ owner: 'o', repo: 'r', page: 1 });

    expect(mocks.listForRepo).toHaveBeenCalledTimes(6);
    expect(result.data?.issues).toEqual([]);
    expect(result.data?.pagination).toMatchObject({
      currentPage: 6,
      nextPage: 7,
      requestedPage: 1,
      skippedPullRequestPages: 5,
      providerPagesFetched: 6,
    });
    expect(result.data?.warnings).toContain(
      'This provider page contained only pull requests (the GitHub issues endpoint returns both; PRs are filtered out) — follow pagination.nextPage. The useful-page scan reached its 5-page skip budget.'
    );
  });
});
