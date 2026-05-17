import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types.js';
import type { FileContentQuery } from '@octocodeai/octocode-core';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import type { ToolExecutionArgs } from '../../types/execution.js';
import { handleCatchError, createSuccessResult } from '../utils.js';
import { isCloneEnabled } from '../../serverConfig.js';
import { fetchDirectoryContents } from '../../github/directoryFetch.js';
import { resolveDefaultBranch } from '../../github/client.js';
import { LOCAL_TOOL_LIST } from '../../hints/localToolUsageHints.js';
import { countSerializedChars } from '../../utils/response/charSavings.js';
import {
  mapFileContentProviderResult,
  mapFileContentToolQuery,
} from '../providerMappers.js';
import {
  createLazyProviderContext,
  createProviderExecutionContext,
  executeProviderOperation,
  providerSupports,
} from '../providerExecution.js';

const DIRECTORY_FETCH_HINTS: string[] = [
  'Directory fetched and saved to disk.',
  'Use `localPath` as the `path` parameter for local tools:',
  ...LOCAL_TOOL_LIST,
  'Tip: start with localViewStructure to explore the fetched directory.',
];

const DIRECTORY_CACHE_HIT_HINT =
  'Served from 24-hour cache (no network call). To force refresh, wait for expiry or manually delete the localPath.';

const DIRECTORY_KEYS_PRIORITY = [
  'resolvedBranch',
  'localPath',
  'fileCount',
  'totalSize',
  'files',
  'cached',
  'expiresAt',
  'error',
];

const FILE_KEYS_PRIORITY = [
  'content',
  'resolvedBranch',
  'pagination',
  'isPartial',
  'startLine',
  'endLine',
  'lastModified',
  'lastModifiedBy',
  'matchLocations',
  'error',
];

export async function fetchMultipleGitHubFileContents(
  args: ToolExecutionArgs<FileContentQuery>
): Promise<CallToolResult> {
  const { queries, authInfo, responseCharOffset, responseCharLength } = args;
  const getProviderContext = createLazyProviderContext(authInfo);

  const hasDirectoryQuery = queries.some(q => q.type === 'directory');
  const hasFileQuery = queries.some(q => q.type !== 'directory');

  const keysPriority =
    hasDirectoryQuery && !hasFileQuery
      ? DIRECTORY_KEYS_PRIORITY
      : FILE_KEYS_PRIORITY;

  return executeBulkOperation(
    queries,
    async (query: FileContentQuery, _index: number) => {
      try {
        const providerContext = getProviderContext();

        if (query.type === 'directory') {
          return handleDirectoryFetch(query, authInfo, providerContext);
        }

        return handleFileFetch(query, providerContext);
      } catch (error) {
        return handleCatchError(error, query);
      }
    },
    {
      toolName: TOOL_NAMES.GITHUB_FETCH_CONTENT,
      keysPriority,
      responseCharOffset,
      responseCharLength,
    }
  );
}

async function handleDirectoryFetch(
  query: FileContentQuery,
  authInfo: AuthInfo | undefined,
  providerContext: ReturnType<typeof createProviderExecutionContext>
) {
  if (!isCloneEnabled()) {
    return handleCatchError(
      new Error(
        'Directory fetch requires ENABLE_LOCAL=true and ENABLE_CLONE=true. ' +
          'Directory mode saves files to disk using the same cache as githubCloneRepo.'
      ),
      query,
      'Clone not enabled',
      TOOL_NAMES.GITHUB_FETCH_CONTENT
    );
  }

  if (!providerSupports(providerContext, 'fetchDirectoryToDisk')) {
    return handleCatchError(
      new Error(
        'Directory fetch (type: "directory") is only available with the GitHub provider. ' +
          'Use file mode (type: "file") instead.'
      ),
      query,
      'Provider not supported',
      TOOL_NAMES.GITHUB_FETCH_CONTENT
    );
  }

  const branch =
    query.branch ??
    (await resolveDefaultBranch(query.owner, query.repo, authInfo));

  const result = await fetchDirectoryContents(
    query.owner,
    query.repo,
    String(query.path),
    branch,
    authInfo,
    Boolean(query.forceRefresh)
  );

  const resultData: Record<string, unknown> = {
    localPath: result.localPath,
    fileCount: result.fileCount,
    totalSize: result.totalSize,
    files: result.files,
    ...(result.cached ? { cached: true } : {}),
    ...(query.branch !== result.branch
      ? { resolvedBranch: result.branch }
      : {}),
  };

  const hints = [...DIRECTORY_FETCH_HINTS];
  if (result.cached) {
    hints.unshift(DIRECTORY_CACHE_HIT_HINT);
  }

  return createSuccessResult(
    query,
    resultData,
    true,
    TOOL_NAMES.GITHUB_FETCH_CONTENT,
    {
      extraHints: hints,
      rawResponse: result.totalSize || countSerializedChars(result),
    }
  );
}

async function handleFileFetch(
  query: FileContentQuery,
  providerContext: ReturnType<typeof createProviderExecutionContext>
) {
  const providerResult = await executeProviderOperation(query, () =>
    providerContext.provider.getFileContent(mapFileContentToolQuery(query))
  );

  if (providerResult.ok === false) {
    return providerResult.result;
  }

  const resultData = mapFileContentProviderResult(
    providerResult.response.data,
    query
  );

  const hasContent = Boolean(
    providerResult.response.data.content &&
    providerResult.response.data.content.length > 0
  );

  const paginationHints = providerResult.response.hints || [];
  const isLarge = providerResult.response.data.size > 50000;
  const isPartial = providerResult.response.data.isPartial;
  const endLine = providerResult.response.data.endLine;

  return createSuccessResult(
    query,
    resultData,
    hasContent,
    TOOL_NAMES.GITHUB_FETCH_CONTENT,
    {
      hintContext: { isLarge, isPartial, endLine },
      extraHints: paginationHints,
      rawResponse: providerResult.response.rawResponseChars,
    }
  );
}
