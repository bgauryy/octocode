import { describe, expect, it, vi, beforeEach } from 'vitest';

const mockSearchPullRequests = vi.fn();

vi.mock('../../../octocode-tools-core/src/tools/providerExecution.js', () => ({
  createProviderExecutionContext: vi.fn(() => ({
    provider: {
      searchPullRequests: mockSearchPullRequests,
    },
  })),
  createLazyProviderContext: vi.fn(() =>
    vi.fn(() => ({
      provider: {
        searchPullRequests: mockSearchPullRequests,
      },
    }))
  ),
  executeProviderOperation: vi.fn(),
}));

import {
  createLazyProviderContext,
  executeProviderOperation,
} from '../../../octocode-tools-core/src/tools/providerExecution.js';
import { searchMultipleGitHubPullRequests } from '../../../octocode-tools-core/src/tools/github_search_pull_requests/execution.js';

function getFirstText(
  result: Awaited<ReturnType<typeof searchMultipleGitHubPullRequests>>
): string {
  const first = result.content?.[0];
  return first && 'text' in first && typeof first.text === 'string'
    ? first.text
    : '';
}

describe('github_search_pull_requests/execution', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('returns validation error when no valid search parameters are provided', async () => {
    const result = await searchMultipleGitHubPullRequests({
      queries: [
        {
          id: 'pr_exec_2',
          state: 'open',
          draft: false,
          mainResearchGoal: 'test',
          researchGoal: 'test',
          reasoning: 'test',
        },
      ],
      authInfo: undefined,
      sessionId: undefined,
    });

    expect(getFirstText(result)).toContain(
      'At least one valid search parameter, filter, or PR number is required.'
    );
    expect(createLazyProviderContext).toHaveBeenCalledTimes(1);
    expect(executeProviderOperation).not.toHaveBeenCalled();
    expect(mockSearchPullRequests).not.toHaveBeenCalled();
  });
});
