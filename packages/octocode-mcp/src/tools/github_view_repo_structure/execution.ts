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
import { isVerbose } from '../../scheme/verbosity.js';
import type { WithVerbosity } from '../../scheme/localSchemaOverlay.js';
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
        const escalationHints: string[] =
          hasContent && query.owner && query.repo
            ? [
                `Structure complete — use githubSearchCode(owner="${query.owner}", repo="${query.repo}") to find patterns, or githubGetFileContent to read specific files.`,
              ]
            : [];
        const branchHints: string[] = branchFallback
          ? [
              `WARNING: Branch '${String((branchFallback as { requestedBranch: string }).requestedBranch)}' not found. Showing '${String((branchFallback as { actualBranch: string }).actualBranch)}' (default branch). Re-query with the correct branch name if branch-specific results are required.`,
            ]
          : [];
        const entryCount = Object.values(filteredStructure).reduce(
          (sum, entry) => sum + entry.files.length + entry.folders.length,
          0
        );

        const summary = (
          resultData as {
            summary?: { truncated?: boolean; filtered?: boolean };
          }
        ).summary;
        const wasTruncated = Boolean(summary?.truncated);
        const wasFiltered = Boolean(summary?.filtered);
        const truncatedReasons: string[] = [];
        if (wasTruncated) {
          truncatedReasons.push(
            `Tree truncated at depth=${query.depth ?? 'default'}; re-query with a deeper depth or a more specific path to see the rest.`
          );
        }
        if (wasFiltered) {
          truncatedReasons.push(
            'Some entries were filtered (e.g. ignored paths); re-query with a narrower path to inspect them directly.'
          );
        }

        const shaped = applyGithubViewRepoStructureVerbosity(
          {
            data: resultData as Record<string, unknown>,
            entryCount,
            summary,
            extraHints: [...apiHints, ...escalationHints],
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
              depth: query.depth,
              branch: query.branch,
              flagFiles: Object.values(filteredStructure).flatMap(entry =>
                entry.files.filter(f =>
                  /(Mode|Config|Flag|Feature)\.[A-Za-z0-9]+$/.test(f)
                )
              ),
            },
            prefixHints: branchHints,
            extraHints: shaped.extraHints,
            evidence: {
              kind: 'structure',
              answerReady: hasContent,
              complete: hasContent && !wasTruncated,
              ...(truncatedReasons.length > 0
                ? { reason: truncatedReasons.join(' ') }
                : {}),
            },
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
      peerHints: true,
      peerEvidence: true,
    }
  );
}

export function applyGithubViewRepoStructureVerbosity(
  input: {
    data: Record<string, unknown>;
    entryCount: number;
    summary: { truncated?: boolean; filtered?: boolean } | undefined;
    extraHints: string[];
  },
  query: PartialRepoStructureQuery
): { data: Record<string, unknown>; extraHints: string[] } {
  const queryWithVerbosity = query as WithVerbosity<typeof query>;
  const nextPathHints = buildNextPathHints(
    (input.data as { structure?: unknown }).structure,
    input.entryCount,
    Boolean(input.summary?.truncated)
  );

  if (!isVerbose(queryWithVerbosity)) {
    const {
      resolvedBranch: _rb,
      branchFallback: _bf,
      ...coreData
    } = input.data as Record<string, unknown>;
    void _rb;
    void _bf;
    return {
      data: coreData,
      extraHints: [...nextPathHints, ...input.extraHints],
    };
  }

  return {
    data: input.data,
    extraHints: [...nextPathHints, ...input.extraHints],
  };
}
