import type { CallToolResult } from '@modelcontextprotocol/server';
import type { ProcessedBulkResult } from '../../types/toolResults.js';
import type { ToolExecutionArgs } from '../../types/execution.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import { createLazyProviderContext } from '../providerExecution.js';
import { searchGitHubCode } from '../github_search_code/execution.js';
import { searchGitHubRepos } from '../github_search_repos/execution.js';
import { exploreRepositoryStructure } from '../github_view_repo_structure/execution.js';
import { executeWithToolBoundary } from '../executionGuard.js';
import { GITHUB_SEARCH_TOOL_NAME } from '../toolNames.js';
import { buildGitHubSearchFinalizer } from './finalizer.js';
import { GitHubSearchQuerySchema, type GitHubSearchQuery } from './scheme.js';

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
  const { operation, ...input } = query;
  switch (operation) {
    case 'code': {
      const { pageSize, ...codeInput } = input as Record<string, unknown> & {
        pageSize?: number;
      };
      const legacyInput = { ...codeInput, limit: pageSize };
      return searchGitHubCode(
        legacyInput as Parameters<typeof searchGitHubCode>[0],
        { ...args, queries: [legacyInput] } as Parameters<
          typeof searchGitHubCode
        >[1],
        getProviderContext
      );
    }
    case 'repositories': {
      const { topics, pageSize, ...repositoryInput } = input as Record<
        string,
        unknown
      > & {
        topics?: string[];
        pageSize?: number;
      };
      const legacyInput = {
        ...repositoryInput,
        topicsToSearch: topics,
        limit: pageSize,
      };
      return searchGitHubRepos(
        legacyInput as Parameters<typeof searchGitHubRepos>[0],
        {
          ...args,
          queries: [legacyInput],
        } as Parameters<typeof searchGitHubRepos>[1],
        getProviderContext
      );
    }
    case 'tree': {
      const { pageSize, ...treeInput } = input as Record<string, unknown> & {
        pageSize?: number;
      };
      return exploreRepositoryStructure(
        {
          ...treeInput,
          itemsPerPage: pageSize,
        } as Parameters<typeof exploreRepositoryStructure>[0],
        {
          ...args,
          queries: [{ ...treeInput, itemsPerPage: pageSize }],
        } as Parameters<typeof exploreRepositoryStructure>[1],
        getProviderContext
      );
    }
    default:
      throw new Error('Unsupported ghSearch operation');
  }
}
