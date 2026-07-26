import { beforeEach, describe, expect, it, vi } from 'vitest';

// Mock the shared executor so we can verify which `type` is injected
const mockSearchMultiple = vi.fn();
vi.mock('../../../src/tools/github_search_pull_requests/execution.js', () => ({
  searchMultipleGitHubPullRequests: (...args: unknown[]) =>
    mockSearchMultiple(...args),
}));

import {
  searchMultipleGitHubPullRequestsSplit,
  searchMultipleGitHubIssues,
  searchMultipleGitHubCommits,
  listMultipleGitHubReleases,
} from '../../../src/tools/github_search_pull_requests/splitExecutions.js';

const SUCCESS_RESULT = {
  content: [{ type: 'text', text: 'ok' }],
  isError: false,
};

function baseArgs(queries: Record<string, unknown>[]) {
  return { queries } as never;
}

describe('splitExecutions — type injection', () => {
  beforeEach(() => {
    mockSearchMultiple.mockReset();
    mockSearchMultiple.mockResolvedValue(SUCCESS_RESULT);
  });

  it('searchMultipleGitHubPullRequestsSplit injects type:"prs"', async () => {
    await searchMultipleGitHubPullRequestsSplit(
      baseArgs([{ owner: 'facebook', repo: 'react' }])
    );

    expect(mockSearchMultiple).toHaveBeenCalledOnce();
    const calledWith = mockSearchMultiple.mock.calls[0]![0];
    expect(calledWith.queries[0].type).toBe('prs');
  });

  it('searchMultipleGitHubIssues injects type:"issues"', async () => {
    await searchMultipleGitHubIssues(
      baseArgs([{ owner: 'microsoft', repo: 'TypeScript' }])
    );

    const calledWith = mockSearchMultiple.mock.calls[0]![0];
    expect(calledWith.queries[0].type).toBe('issues');
  });

  it('searchMultipleGitHubCommits injects type:"commits"', async () => {
    await searchMultipleGitHubCommits(
      baseArgs([{ owner: 'vercel', repo: 'next.js' }])
    );

    const calledWith = mockSearchMultiple.mock.calls[0]![0];
    expect(calledWith.queries[0].type).toBe('commits');
  });

  it('listMultipleGitHubReleases injects type:"releases"', async () => {
    await listMultipleGitHubReleases(
      baseArgs([{ owner: 'vercel', repo: 'next.js' }])
    );

    const calledWith = mockSearchMultiple.mock.calls[0]![0];
    expect(calledWith.queries[0].type).toBe('releases');
  });

  it('preserves existing query fields when injecting type', async () => {
    await searchMultipleGitHubPullRequestsSplit(
      baseArgs([{ owner: 'facebook', repo: 'react', state: 'open', limit: 5 }])
    );

    const calledWith = mockSearchMultiple.mock.calls[0]![0];
    expect(calledWith.queries[0].owner).toBe('facebook');
    expect(calledWith.queries[0].repo).toBe('react');
    expect(calledWith.queries[0].state).toBe('open');
    expect(calledWith.queries[0].limit).toBe(5);
    expect(calledWith.queries[0].type).toBe('prs');
  });

  it('handles multiple queries in the batch', async () => {
    await searchMultipleGitHubIssues(
      baseArgs([
        { owner: 'microsoft', repo: 'TypeScript' },
        { owner: 'facebook', repo: 'react' },
      ])
    );

    const calledWith = mockSearchMultiple.mock.calls[0]![0];
    expect(calledWith.queries).toHaveLength(2);
    expect(calledWith.queries[0].type).toBe('issues');
    expect(calledWith.queries[1].type).toBe('issues');
  });

  it('does not mutate the original args object', async () => {
    const original = { queries: [{ owner: 'facebook', repo: 'react' }] };
    await searchMultipleGitHubPullRequestsSplit(original as never);
    expect(original.queries[0]).not.toHaveProperty('type');
  });

  it('preserves non-queries fields from the args object', async () => {
    const args = {
      queries: [{ owner: 'facebook', repo: 'react' }],
      serverInfo: { name: 'octocode' },
    } as never;
    await searchMultipleGitHubPullRequestsSplit(args);
    const calledWith = mockSearchMultiple.mock.calls[0]![0];
    expect(calledWith.serverInfo).toEqual({ name: 'octocode' });
  });

  it('returns the result from the delegate', async () => {
    const customResult = { content: [{ text: 'custom' }], isError: false };
    mockSearchMultiple.mockResolvedValue(customResult);

    const result = await searchMultipleGitHubPullRequestsSplit(
      baseArgs([{ owner: 'facebook', repo: 'react' }])
    );
    expect(result).toBe(customResult);
  });
});
