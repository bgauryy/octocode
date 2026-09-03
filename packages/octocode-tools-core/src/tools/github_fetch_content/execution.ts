import type { CallToolResult, AuthInfo } from '@modelcontextprotocol/server';
import type { z } from 'zod';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import type { ToolExecutionArgs } from '../../types/execution.js';
import {
  handleCatchError,
  createSuccessResult,
  createErrorResult,
  safeParseOrError,
} from '../utils.js';
import { FileContentQueryLocalSchema } from './scheme.js';
import type { MinifyMode } from '../../scheme/fields.js';
import {
  fetchDirectoryContents,
  fetchFileContentToDisk,
} from '../../github/directoryFetch.js';
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
import { getConfigSync } from '@octocodeai/config';

type FileContentInputQuery = z.input<typeof FileContentQueryLocalSchema>;

type PartialFileContentQuery = z.output<typeof FileContentQueryLocalSchema> & {
  minify: MinifyMode;
};

export async function fetchMultipleGitHubFileContents(
  args: ToolExecutionArgs<FileContentInputQuery>
): Promise<CallToolResult> {
  const { queries, authInfo } = args;
  const getProviderContext = createLazyProviderContext(authInfo);

  return executeBulkOperation(
    queries,
    async (query: FileContentInputQuery, _index: number) => {
      try {
        const parsed = safeParseOrError(FileContentQueryLocalSchema, query, {
          prefix: false,
        });
        if (parsed.ok === false) {
          return parsed.error;
        }

        const parsedData = parsed.data as z.output<
          typeof FileContentQueryLocalSchema
        >;

        // Resolve the effective minify mode here rather than via a schema
        // default. `fullContent` promises the whole file verbatim, so it
        // defaults to 'none' (unminified — otherwise 'standard' would silently
        // strip comments/blank lines and "Returns the whole file" would be a
        // lie); every other read defaults to 'standard'. An explicit minify
        // always wins. Resolving here (not at the schema) is what lets us tell
        // "caller omitted minify" from "caller chose standard": inputSchema is
        // parsed upstream before execution, which would apply a schema default.
        const resolvedMinify: MinifyMode =
          parsedData.minify ??
          (parsedData.fullContent === true ? 'none' : 'standard');
        const effectiveQuery: PartialFileContentQuery = {
          ...parsedData,
          minify: resolvedMinify,
        };

        if (effectiveQuery.type === 'directory') {
          const capabilityError =
            getDirectoryMaterializationCapabilityError(effectiveQuery);
          if (capabilityError) return capabilityError;
        }

        const providerContext = getProviderContext();

        if (effectiveQuery.type === 'directory') {
          return handleDirectoryFetch(
            effectiveQuery,
            authInfo,
            providerContext
          );
        }

        return handleFileFetch(effectiveQuery, authInfo, providerContext);
      } catch (error) {
        return handleCatchError(
          error,
          query,
          undefined,
          TOOL_NAMES.GITHUB_FETCH_CONTENT
        );
      }
    },
    {
      toolName: TOOL_NAMES.GITHUB_FETCH_CONTENT,
      finalize: buildGithubFetchContentFinalizer<FileContentInputQuery>(),
    },
    args
  );
}

function getDirectoryMaterializationCapabilityError(
  query: PartialFileContentQuery
) {
  const config = getConfigSync();
  if (!config.local.enabled) {
    return createErrorResult(
      'Directory fetch requires local materialization. Set ENABLE_LOCAL=true to use type: "directory".',
      query,
      { extra: { errorCode: 'localToolsDisabled' }, rawResponse: 0 }
    );
  }
  if (!config.local.enableClone) {
    return createErrorResult(
      'Directory fetch requires clone/materialization support. Set ENABLE_CLONE=true to use type: "directory".',
      query,
      { extra: { errorCode: 'cloneDisabled' }, rawResponse: 0 }
    );
  }
  if (config.storage.mode === 'memory') {
    return createErrorResult(
      'Directory fetch requires persistent local materialization. Set storage.mode="persistent" or OCTOCODE_STORAGE_MODE=persistent to use type: "directory".',
      query,
      {
        extra: { errorCode: 'persistentStorageDisabled' },
        rawResponse: 0,
      }
    );
  }
  return undefined;
}

async function handleDirectoryFetch(
  query: PartialFileContentQuery,
  authInfo: AuthInfo | undefined,
  providerContext: ReturnType<typeof createProviderExecutionContext>
) {
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

  const hasSubdirectories = (result.skipped?.nonFile ?? 0) > 0;
  const skippedSummary = result.skipped
    ? Object.fromEntries(
        Object.entries(result.skipped).filter(([, v]) => v > 0)
      )
    : undefined;

  const location: Record<string, unknown> = {
    kind: 'directory',
    localPath: result.localPath,
    repoRoot: result.repoRoot,
    source: 'treeFetch',
    cached: result.cached,
    complete: result.complete,
    verified: result.verified,
    ...(result.commitSha ? { commitSha: result.commitSha } : {}),
    ...(hasSubdirectories ? { hasSubdirectories: true } : {}),
    ...(skippedSummary && Object.keys(skippedSummary).length > 0
      ? { skippedSummary }
      : {}),
    owner: query.owner,
    repo: query.repo,
  };

  const next: Record<string, unknown> = {
    viewStructure: {
      tool: 'localSearch',
      query: { operation: 'tree', path: result.localPath },
    },
    // When subdirectories were skipped, provide a pre-filled clone hint so
    // agents can escalate to a complete local copy without constructing the
    // call manually.
    ...(hasSubdirectories
      ? {
          escalateToClone: {
            tool: 'ghCloneRepo',
            why: 'nonFile skips indicate subdirectories were not fetched; clone for full coverage',
            query: {
              owner: query.owner,
              repo: query.repo,
              ...(query.branch ? { branch: query.branch } : {}),
              ...(query.path ? { sparsePath: String(query.path) } : {}),
            },
          },
        }
      : {}),
  };

  const resultData: Record<string, unknown> = {
    localPath: result.localPath,
    repoRoot: result.repoRoot,
    fileCount: result.fileCount,
    totalSize: result.totalSize,
    complete: result.complete,
    verified: result.verified,
    ...(result.commitSha ? { commitSha: result.commitSha } : {}),
    directoryEntryCount: result.directoryEntryCount,
    eligibleFileCount: result.eligibleFileCount,
    savedFileCount: result.savedFileCount,
    // All-zeros skip map is noise (and duplicates location.skippedSummary's
    // absence); emit the full breakdown only when something was skipped.
    ...(skippedSummary && Object.keys(skippedSummary).length > 0
      ? { skipped: result.skipped }
      : {}),
    limits: result.limits,
    ...(result.warnings ? { warnings: result.warnings } : {}),
    files: result.files,
    ...(result.cached ? { cached: true } : {}),
    ...(query.branch !== result.branch
      ? { resolvedBranch: result.branch }
      : {}),
    location,
    next,
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
  authInfo: AuthInfo | undefined,
  providerContext: ReturnType<typeof createProviderExecutionContext>
) {
  const providerResult = await executeProviderOperation(query, () =>
    providerContext.provider.getFileContent(mapFileContentToolQuery(query))
  );

  if (providerResult.ok === false) {
    return providerResult.result;
  }

  const materialized =
    query.fullContent === true &&
    query.minify === 'none' &&
    getConfigSync().storage.mode === 'persistent'
      ? await materializeExactFile(query, authInfo)
      : undefined;

  const resultData = {
    ...mapFileContentProviderResult(providerResult.response.data, query),
    ...(materialized
      ? {
          localPath: materialized.localPath,
          repoRoot: materialized.repoRoot,
          cached: materialized.cached,
          commitSha: materialized.commitSha,
          // Provenance: always name the ref that was actually served, so the
          // answer is citable even when it equals the requested branch.
          ...(materialized.branch
            ? { resolvedBranch: materialized.branch }
            : {}),
        }
      : {}),
  };

  const hasContent = Boolean(
    providerResult.response.data.matchNotFound === true ||
    (providerResult.response.data.content &&
      providerResult.response.data.content.length > 0)
  );

  return createSuccessResult(
    query,
    resultData,
    hasContent,
    TOOL_NAMES.GITHUB_FETCH_CONTENT,
    {
      rawResponse: providerResult.response.rawResponseChars,
    }
  );
}

async function materializeExactFile(
  query: PartialFileContentQuery,
  authInfo: AuthInfo | undefined
) {
  if (!query.owner || !query.repo || typeof query.path !== 'string') {
    return undefined;
  }

  const branch =
    query.branch ??
    (await resolveDefaultBranch(query.owner, query.repo, authInfo));

  return fetchFileContentToDisk(
    query.owner,
    query.repo,
    query.path,
    branch,
    authInfo,
    Boolean(query.forceRefresh)
  );
}
