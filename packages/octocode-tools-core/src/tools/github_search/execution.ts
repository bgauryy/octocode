import type { CallToolResult } from '@modelcontextprotocol/server';
import type { ProcessedBulkResult } from '../../types/toolResults.js';
import type { ToolExecutionArgs } from '../../types/execution.js';
import { executeBulkOperation } from '../../utils/response/bulk/response.js';
import { createLazyProviderContext } from '../providerExecution.js';
import { searchGitHubCode } from '../github_search_code/execution.js';
import { searchGitHubRepos } from '../github_search_repos/execution.js';
import { exploreRepositoryStructure } from '../github_view_repo_structure/execution.js';
import { executeWithToolBoundary } from '../executionGuard.js';
import { GITHUB_SEARCH_TOOL_NAME } from '@octocodeai/octocode-core/schema';
import { buildGitHubSearchFinalizer } from './finalizer.js';
import {
  GitHubSearchQuerySchema,
  type GitHubSearchQuery,
} from '@octocodeai/octocode-core/schema';

export async function executeGitHubSearch(
  args: ToolExecutionArgs<GitHubSearchQuery>
): Promise<CallToolResult> {
  const getProviderContext = createLazyProviderContext(args.authInfo);
  return executeBulkOperation(
    args.queries,
    query =>
      executeWithToolBoundary({
        toolName: GITHUB_SEARCH_TOOL_NAME,
        query,
        contextMessage: 'ghSearch execution failed',
        execute: async () => {
          const parsed = GitHubSearchQuerySchema.safeParse(query);
          if (!parsed.success) throw parsed.error;
          return runOperation(parsed.data, args, getProviderContext);
        },
      }),
    {
      toolName: GITHUB_SEARCH_TOOL_NAME,
      finalize: buildGitHubSearchFinalizer(),
    },
    args
  );
}

async function runOperation(
  query: GitHubSearchQuery,
  args: ToolExecutionArgs<GitHubSearchQuery>,
  getProviderContext: ReturnType<typeof createLazyProviderContext>
): Promise<ProcessedBulkResult> {
  switch (query.operation) {
    case 'code':
      return searchGitHubCode(query, args, getProviderContext);
    case 'repositories':
      return searchGitHubRepos(query, args, getProviderContext);
    case 'tree':
      return exploreRepositoryStructure(query, args, getProviderContext);
    default:
      throw new Error('Unsupported ghSearch operation');
  }
}
