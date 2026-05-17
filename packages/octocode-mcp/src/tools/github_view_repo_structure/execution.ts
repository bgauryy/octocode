import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type {
  GitHubViewRepoStructureQuery,
  GitHubViewRepoStructureToolResult,
  GitHubRepoStructureDirectoryEntry,
} from '@octocodeai/octocode-core';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import type { ToolExecutionArgs } from '../../types/execution.js';
import { shouldIgnoreFile, shouldIgnoreDir } from '../../utils/file/filters.js';
import { handleCatchError, createSuccessResult } from '../utils.js';
import {
  mapRepoStructureProviderResult,
  mapRepoStructureToolQuery,
} from '../providerMappers.js';
import {
  createLazyProviderContext,
  executeProviderOperation,
} from '../providerExecution.js';

function filterStructure(
  structure: Record<string, GitHubRepoStructureDirectoryEntry>
): Record<string, GitHubRepoStructureDirectoryEntry> {
  const filtered: Record<string, GitHubRepoStructureDirectoryEntry> = {};

  for (const [dirPath, entry] of Object.entries(structure)) {
    const filteredFiles = entry.files.filter(
      fileName => !shouldIgnoreFile(fileName)
    );
    const filteredFolders = entry.folders.filter(
      folderName => !shouldIgnoreDir(folderName)
    );

    if (filteredFiles.length > 0 || filteredFolders.length > 0) {
      filtered[dirPath] = {
        files: filteredFiles,
        folders: filteredFolders,
      };
    }
  }

  return filtered;
}

export async function exploreMultipleRepositoryStructures(
  args: ToolExecutionArgs<GitHubViewRepoStructureQuery>
): Promise<CallToolResult> {
  const { queries, authInfo, responseCharOffset, responseCharLength } = args;
  const getProviderContext = createLazyProviderContext(authInfo);

  return executeBulkOperation(
    queries,
    async (query: GitHubViewRepoStructureQuery, _index: number) => {
      try {
        const currentProviderContext = getProviderContext();
        const projectId = `${query.owner}/${query.repo}`;
        const resolvedBranch =
          query.branch ??
          (await currentProviderContext.provider.resolveDefaultBranch(
            projectId
          ));

        const providerResult = await executeProviderOperation(query, () =>
          currentProviderContext.provider.getRepoStructure(
            mapRepoStructureToolQuery(query, resolvedBranch)
          )
        );

        if (providerResult.ok === false) {
          return providerResult.result;
        }

        const filteredStructure = filterStructure(
          providerResult.response.data.structure
        );
        const hasContent = Object.keys(filteredStructure).length > 0;
        const resultData = mapRepoStructureProviderResult(
          providerResult.response.data,
          query,
          filteredStructure,
          resolvedBranch
        );

        const branchFallback =
          'branchFallback' in resultData
            ? resultData.branchFallback
            : undefined;
        const apiHints = providerResult.response.data.hints || [];
        const branchHints: string[] = branchFallback
          ? [
              `⚠️ IMPORTANT: Branch '${String((branchFallback as { requestedBranch: string }).requestedBranch)}' not found — showing '${String((branchFallback as { actualBranch: string }).actualBranch)}' (default branch). Re-query with the correct branch name if branch-specific results are required.`,
            ]
          : [];
        const entryCount = Object.values(filteredStructure).reduce(
          (sum, entry) => sum + entry.files.length + entry.folders.length,
          0
        );

        return createSuccessResult(
          query,
          resultData,
          hasContent,
          TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
          {
            hintContext: { entryCount },
            prefixHints: branchHints,
            extraHints: apiHints,
            rawResponse: providerResult.response.rawResponseChars,
          }
        );
      } catch (error) {
        return handleCatchError(
          error,
          query,
          'Failed to explore repository structure'
        );
      }
    },
    {
      toolName: TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
      keysPriority: [
        'resolvedBranch',
        'branchFallback',
        'structure',
        'error',
      ] satisfies Array<keyof GitHubViewRepoStructureToolResult>,
      responseCharOffset,
      responseCharLength,
    }
  );
}
