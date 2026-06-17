import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type { z } from 'zod';
import type { GitHubViewRepoStructureQuerySchema } from '@octocodeai/octocode-core/schemas';
import type {
  GitHubViewRepoStructureToolResult,
  GitHubRepoStructureDirectoryEntry,
} from '@octocodeai/octocode-core/extra-types';

type GitHubViewRepoStructureQuery = z.infer<
  typeof GitHubViewRepoStructureQuerySchema
>;
import type { WithOptionalMeta } from '../../types/execution.js';

type PartialRepoStructureQuery = WithOptionalMeta<GitHubViewRepoStructureQuery>;
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import type { ToolExecutionArgs } from '../../types/execution.js';
import { shouldIgnoreFile, shouldIgnoreDir } from '../../utils/file/filters.js';
import { handleCatchError, createSuccessResult } from '../utils.js';
import type { ProcessedBulkResult } from '../../types/toolResults.js';
import {
  mapRepoStructureProviderResult,
  mapRepoStructureToolQuery,
} from '../providerMappers.js';
import {
  createLazyProviderContext,
  executeProviderOperation,
} from '../providerExecution.js';

const CONCISE_TOP_ENTRIES = 5;

function collectAllStructureEntries(structure: unknown): string[] {
  if (!structure || typeof structure !== 'object') return [];
  const folders: string[] = [];
  const files: string[] = [];
  for (const entry of Object.values(structure as Record<string, unknown>)) {
    const e = entry as { files?: unknown; folders?: unknown };
    if (Array.isArray(e.folders))
      for (const f of e.folders)
        if (typeof f === 'string') folders.push(`${f}/`);
    if (Array.isArray(e.files))
      for (const f of e.files) if (typeof f === 'string') files.push(f);
  }
  return [...folders, ...files];
}

function collectTopStructureEntries(
  structure: unknown,
  limit: number
): string[] {
  return collectAllStructureEntries(structure).slice(0, limit);
}

function buildStructureNavigationHint(input: {
  owner?: string;
  repo?: string;
  truncated: boolean;
  hasMore: boolean;
}): string | undefined {
  if (!input.owner || !input.repo) return undefined;

  const prefix =
    input.truncated || input.hasMore
      ? 'Structure page is partial'
      : 'Structure complete';

  return `${prefix} - use ghSearchCode(owner="${input.owner}", repo="${input.repo}") to find patterns, or ghGetFileContent to read specific files.`;
}

function buildNextPathHints(
  structure: unknown,
  entryCount: number,
  truncated: boolean
): string[] {
  if (!truncated) return [];
  const topEntries = collectTopStructureEntries(structure, CONCISE_TOP_ENTRIES);
  if (topEntries.length === 0) return [];
  const more =
    entryCount > topEntries.length
      ? ` (+${entryCount - topEntries.length} more)`
      : '';
  return [`Next paths: ${topEntries.join(', ')}${more}`];
}

function buildStructurePageHint(pagination: {
  currentPage?: number;
  totalPages?: number;
  totalEntries?: number;
  entriesPerPage?: number;
}): string {
  const currentPage = pagination.currentPage ?? 1;
  const totalPages = pagination.totalPages ?? currentPage + 1;
  const totalEntries = pagination.totalEntries;
  const entriesPerPage = pagination.entriesPerPage;
  const visible =
    typeof totalEntries === 'number' && typeof entriesPerPage === 'number'
      ? ` (showing ${Math.min(currentPage * entriesPerPage, totalEntries)} of ${totalEntries})`
      : '';
  return `Page ${currentPage}/${totalPages}${visible}. Next: page=${currentPage + 1}`;
}

function normalizeStructureErrorResult(
  result: ProcessedBulkResult,
  query: PartialRepoStructureQuery
): ProcessedBulkResult {
  const rawError = result.error;
  const apiError =
    typeof rawError === 'object' && rawError !== null
      ? (rawError as { error?: unknown; status?: unknown; type?: unknown })
      : undefined;

  return {
    status: 'error',
    owner: query.owner,
    repo: query.repo,
    path: query.path,
    branch: query.branch,
    error:
      typeof apiError?.error === 'string'
        ? apiError.error
        : typeof rawError === 'string'
          ? rawError
          : 'Failed to explore repository structure',
    ...(typeof apiError?.status === 'number'
      ? { statusCode: apiError.status }
      : {}),
    ...(typeof apiError?.type === 'string' ? { errorType: apiError.type } : {}),
    ...(Array.isArray(result.hints) ? { hints: result.hints } : {}),
  };
}

export function filterStructure(
  structure: Record<string, GitHubRepoStructureDirectoryEntry>
): Record<string, GitHubRepoStructureDirectoryEntry> {
  const filtered: Record<string, GitHubRepoStructureDirectoryEntry> = {};

  for (const [dirPath, entry] of Object.entries(structure)) {
    // Skip top-level entries for directories that should be ignored
    const dirName = dirPath.split('/').pop() ?? dirPath;
    if (dirPath !== '' && dirPath !== '.' && shouldIgnoreDir(dirName)) {
      continue;
    }

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
  args: ToolExecutionArgs<PartialRepoStructureQuery>
): Promise<CallToolResult> {
  const { queries, authInfo } = args;
  const getProviderContext = createLazyProviderContext(authInfo);

  return executeBulkOperation(
    queries,
    async (query: PartialRepoStructureQuery, _index: number) => {
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
          return normalizeStructureErrorResult(providerResult.result, query);
        }

        const originalHasContent =
          Object.keys(providerResult.response.data.structure ?? {}).length > 0;
        const filteredStructure = filterStructure(
          providerResult.response.data.structure
        );
        const hasContent = Object.keys(filteredStructure).length > 0;
        const wasFilteredToEmpty = originalHasContent && !hasContent;
        const wasTruncated = Boolean(
          providerResult.response.data.summary?.truncated
        );
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
              `WARNING: Branch '${String((branchFallback as { requestedBranch: string }).requestedBranch)}' not found. Showing '${String((branchFallback as { actualBranch: string }).actualBranch)}' (default branch). Re-query with the correct branch name if branch-specific results are required.`,
            ]
          : [];
        const entryCount = Object.values(filteredStructure).reduce(
          (sum, entry) => sum + entry.files.length + entry.folders.length,
          0
        );

        const pagination = (
          resultData as {
            pagination?: {
              hasMore?: boolean;
              currentPage?: number;
              totalPages?: number;
              totalEntries?: number;
              entriesPerPage?: number;
            };
          }
        ).pagination;
        const hasMorePages = Boolean(pagination?.hasMore);

        const navigationHint =
          hasContent && !hasMorePages
            ? buildStructureNavigationHint({
                owner: query.owner,
                repo: query.repo,
                truncated: wasTruncated,
                hasMore: false,
              })
            : undefined;
        const extraHintsForOutput = hasMorePages
          ? [buildStructurePageHint(pagination ?? {})]
          : [...apiHints, ...(navigationHint ? [navigationHint] : [])];

        const truncatedReasons: string[] = [];
        if (hasMorePages) {
          const currentPage = pagination?.currentPage ?? 1;
          const totalPages = pagination?.totalPages;
          truncatedReasons.push(
            `Tree paginated (page ${currentPage}${totalPages ? ` of ${totalPages}` : ''}); use page=${currentPage + 1} to fetch the remaining entries.`
          );
        } else if (wasTruncated) {
          truncatedReasons.push(
            `Tree truncated at maxDepth=${query.maxDepth ?? 'default'}; re-query with a deeper maxDepth or a more specific path to see the rest.`
          );
        }

        const shaped = buildRepoStructureOutput(
          {
            data: resultData as Record<string, unknown>,
            entryCount,
            wasTruncated,
            extraHints: extraHintsForOutput,
          },
          query
        );

        return createSuccessResult(
          query,
          shaped.data,
          hasContent,
          TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
          {
            hintContext: {
              entryCount,
              path: query.path,
              depth: query.maxDepth,
              branch: query.branch,
              wasFilteredToEmpty,
              flagFiles: Object.values(filteredStructure).flatMap(entry =>
                entry.files.filter(f =>
                  /(Mode|Config|Flag|Feature)\.[A-Za-z0-9]+$/.test(f)
                )
              ),
            },
            prefixHints: branchHints,
            extraHints: [...truncatedReasons, ...shaped.extraHints],
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
        'summary',
        'pagination',
        'structure',
        'error',
      ] satisfies Array<keyof GitHubViewRepoStructureToolResult>,
      peerHints: true,
    },
    args
  );
}

export function buildRepoStructureOutput(
  input: {
    data: Record<string, unknown>;
    entryCount: number;
    wasTruncated: boolean;
    extraHints: string[];
  },
  _query: PartialRepoStructureQuery
): { data: Record<string, unknown>; extraHints: string[] } {
  const nextPathHints = buildNextPathHints(
    (input.data as { structure?: unknown }).structure,
    input.entryCount,
    input.wasTruncated
  );

  return {
    data: input.data,
    extraHints: [...nextPathHints, ...input.extraHints],
  };
}
