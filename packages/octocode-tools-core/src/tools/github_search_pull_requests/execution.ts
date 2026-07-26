import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import type { ToolExecutionArgs } from '../../types/execution.js';
import { handleCatchError, safeParseOrError } from '../utils.js';
import { createLazyProviderContext } from '../providerExecution.js';
import { handleReleasesMode } from './execution/releasesMode.js';
import { handleIssuesMode } from './execution/issuesMode.js';
import { handleCommitsMode } from './execution/commitsMode.js';
import { handlePullRequestsMode } from './execution/pullRequestsMode.js';
import { GitHubPullRequestSearchQueryLocalSchema } from './scheme.js';
import type { GitHubPullRequestSearchInput } from './execution/types.js';

export async function searchMultipleGitHubPullRequests(
  args: ToolExecutionArgs<GitHubPullRequestSearchInput>
): Promise<CallToolResult> {
  const { queries, authInfo } = args;
  const getProviderContext = createLazyProviderContext(authInfo);

  return executeBulkOperation(
    queries,
    async (query: GitHubPullRequestSearchInput, _index: number) => {
      try {
        const parsed = safeParseOrError(
          GitHubPullRequestSearchQueryLocalSchema,
          query
        );
        if (parsed.ok === false) {
          return parsed.error;
        }

        // `type` is injected by the split executors (ghSearchCommits/Issues/…),
        // each backed by a FOCUSED schema that strips foreign fields before this
        // runs — so no cross-mode "ignored field" warning machinery is needed.
        const type = (parsed.data as { type?: string }).type;

        if (type === 'releases') {
          return await handleReleasesMode(query, parsed.data, authInfo);
        }

        if (type === 'issues') {
          return await handleIssuesMode(query, parsed.data, authInfo);
        }

        if (type === 'commits') {
          return await handleCommitsMode(query, parsed.data, authInfo);
        }

        return await handlePullRequestsMode(
          query,
          parsed.data,
          getProviderContext
        );
      } catch (error) {
        return handleCatchError(
          error,
          query,
          undefined,
          TOOL_NAMES.GITHUB_PULL_REQUESTS
        );
      }
    },
    {
      toolName: TOOL_NAMES.GITHUB_PULL_REQUESTS,
      keysPriority: [
        'pull_requests',
        'issues',
        'releases',
        'latest',
        'tagName',
        'publishedAt',
        'prerelease',
        'pagination',
        'total_count',
        'error',
      ],
    },
    args
  );
}
