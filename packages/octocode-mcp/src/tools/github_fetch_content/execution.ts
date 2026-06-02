import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types.js';
import type { z } from 'zod/v4';
import type { FileContentQuerySchema } from '@octocodeai/octocode-core/schemas';

type FileContentQuery = z.infer<typeof FileContentQuerySchema>;
import type { WithOptionalMeta } from '../../types/execution.js';

type PartialFileContentQuery = WithOptionalMeta<FileContentQuery>;
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import type { ToolExecutionArgs } from '../../types/execution.js';
import {
  handleCatchError,
  createSuccessResult,
  createErrorResult,
} from '../utils.js';
import { FileContentQueryLocalSchema } from '../../scheme/remoteSchemaOverlay.js';
import { isCloneEnabled } from '../../serverConfig.js';
import { fetchDirectoryContents } from '../../github/directoryFetch.js';
import { resolveDefaultBranch } from '../../github/client.js';
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
import { buildGithubFetchContentFinalizer } from './finalizer.js';

// Re-exported so every tool exposes `apply<Tool>Verbosity` from execution.ts.
export { applyGithubFetchContentVerbosity } from './finalizer.js';

export async function fetchMultipleGitHubFileContents(
  args: ToolExecutionArgs<PartialFileContentQuery>
): Promise<CallToolResult> {
  const { queries, authInfo, responseCharOffset, responseCharLength } = args;
  const getProviderContext = createLazyProviderContext(authInfo);

  return executeBulkOperation(
    queries,
    async (query: PartialFileContentQuery, _index: number) => {
      try {
        // Per-query extraction-mode mutex. The bulk envelope is relaxed (so one
        // malformed query never rejects the whole batch at MCP validation);
        // enforce the fullContent/matchString/lineRange mutex here instead so a
        // bad query errors on its own while valid siblings still run.
        const validated = FileContentQueryLocalSchema.safeParse(query);
        if (!validated.success) {
          const messages = validated.error.issues
            .map(i => i.message)
            .join('; ');
          return createErrorResult(messages, query);
        }

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
      responseCharOffset,
      responseCharLength,
      peerHints: true,
      peerEvidence: true,
      finalize: buildGithubFetchContentFinalizer<PartialFileContentQuery>(),
    }
  );
}

async function handleDirectoryFetch(
  query: PartialFileContentQuery,
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

  if (!query.owner || !query.repo) {
    return createErrorResult(
      'Directory fetch requires both owner and repo.',
      query,
      {
        rawResponse: 0,
      }
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

  return createSuccessResult(
    query,
    resultData,
    true,
    TOOL_NAMES.GITHUB_FETCH_CONTENT,
    {
      rawResponse: result.totalSize ?? countSerializedChars(result),
    }
  );
}

async function handleFileFetch(
  query: PartialFileContentQuery,
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

  return createSuccessResult(
    query,
    resultData,
    hasContent,
    TOOL_NAMES.GITHUB_FETCH_CONTENT,
    {
      rawResponse: providerResult.response.rawResponseChars,
      // Path drives the non-canonical (examples/__tests__/docs/fixtures)
      // warning in hints.hasResults. isPartial/endLine keep continuation
      // hints working alongside.
      hintContext: {
        path: query.path,
        branch: query.branch,
        isPartial: (resultData as { isPartial?: boolean }).isPartial,
        endLine: (resultData as { endLine?: number }).endLine,
      },
    }
  );
}
