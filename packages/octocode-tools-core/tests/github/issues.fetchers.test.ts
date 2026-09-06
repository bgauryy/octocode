import { beforeEach, describe, expect, it, vi } from 'vitest';
import { clearAllCache } from '../../src/utils/http/cache/management.js';

const issueGet = vi.fn();
const listComments = vi.fn();

vi.mock('../../src/github/client.js', () => ({
  resolveCacheAuthFingerprint: async () => 'issue-fetchers-test',
  getOctokit: vi.fn(async () => ({
    rest: { issues: { get: issueGet, listComments } },
  })),
}));

const { fetchIssueByNumber } =
  await import('../../src/github/issues/fetchers.js');

describe('fetchIssueByNumber', () => {
  beforeEach(() => {
    clearAllCache();
    issueGet.mockReset();
    listComments.mockReset();
  });

  it('redirects pull requests with a runnable canonical tool continuation', async () => {
    issueGet.mockResolvedValue({ data: { pull_request: { url: 'pr' } } });

    const result = await fetchIssueByNumber({
      owner: 'octocode-ai',
      repo: 'octocode',
      issueNumber: 42,
    });

    expect(result).toEqual({
      error:
        'Issue #42 is a pull request; use ghGetHistoryItem operation:"pullRequest" with number:42.',
      type: 'http',
      hints: [
        'Retry with ghGetHistoryItem { operation: "pullRequest", owner: "octocode-ai", repo: "octocode", number: 42 }.',
      ],
    });
    expect(JSON.stringify(result)).not.toContain('type:\\"prs\\"');
  });

  it('reports the schema page limit instead of emitting issue comment page 1001', async () => {
    issueGet.mockResolvedValue({
      data: {
        number: 42,
        title: 'Issue',
        state: 'open',
        user: { login: 'octocat' },
        labels: [],
        created_at: '2026-01-01T00:00:00Z',
        updated_at: '2026-01-01T00:00:00Z',
      },
    });
    listComments.mockResolvedValue({
      data: [],
      headers: {
        link: '<https://api.github.test/comments?page=1001>; rel="next"',
      },
    });

    const result = await fetchIssueByNumber({
      owner: 'octocode-ai',
      repo: 'octocode',
      issueNumber: 42,
      content: { comments: { discussion: true } },
      commentPage: 1000,
      itemsPerPage: 1,
    });
    const comments = result.data.issues[0]?.contentPagination?.comments;

    expect(comments).toMatchObject({
      currentPage: 1000,
      hasMore: true,
      terminalLimit: true,
      continuationUnavailable: {
        reason: 'schemaPageLimit',
        maxPage: 1000,
      },
    });
    expect(comments).not.toHaveProperty('nextCommentPage');
  });
});
