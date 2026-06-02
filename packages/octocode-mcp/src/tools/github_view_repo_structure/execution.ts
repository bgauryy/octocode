import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type { z } from 'zod/v4';
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
import {
  isConcise,
  isCompact,
  compactTrimHints,
  makeAdvisoryPredicate,
} from '../../scheme/verbosity.js';
import type { WithVerbosity } from '../../scheme/localSchemaOverlay.js';

/** Advisory hints githubViewRepoStructure emits; stripped under compact.
 * Substring-OR, case-insensitive. */
const isAdvisoryViewRepoStructureHint = makeAdvisoryPredicate([
  'tree may report',
  'truncated at depth',
  'monorepo',
  'sibling config',
  'sibling files',
]);
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

/** How many entry names concise samples into the `top:` hint for drill-down. */
const CONCISE_TOP_ENTRIES = 5;

/**
 * Sample top entry names from a structure map for the concise `top:` hint.
 * Folders first (suffixed `/`), then files — folders are the more useful
 * drill targets during recon.
 */
function collectTopStructureEntries(
  structure: unknown,
  limit: number
): string[] {
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
  return [...folders, ...files].slice(0, limit);
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
  const { queries, authInfo, responseCharOffset, responseCharLength } = args;
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
            extraHints: apiHints,
          },
          query
        );

        return createSuccessResult(
          query,
          shaped.data,
          hasContent,
          TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
          {
            // Pass path/depth/branch so empty-listing hints can name the
            // exact location that came back empty and suggest a concrete
            // probe (parent dir, depth=2, different branch).
            // flagFiles lets hints.hasResults surface feature-flag /
            // *Mode/*Config files that gate the implementation a direct
            // search would miss.
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
      responseCharOffset,
      responseCharLength,
      peerHints: true,
      peerEvidence: true,
    }
  );
}

/**
 * Per-tool verbosity shaping for githubViewRepoStructure. Under concise, replaces
 * the full `structure` payload with `{path, summary, entryCount}` + a
 * drill-back hint. Under compact, advisory hints are trimmed to 2. Basic /
 * omitted: passthrough.
 */
export function applyGithubViewRepoStructureVerbosity(
  input: {
    data: Record<string, unknown>;
    entryCount: number;
    summary: { truncated?: boolean; filtered?: boolean } | undefined;
    extraHints: string[];
  },
  query: PartialRepoStructureQuery
): { data: Record<string, unknown>; extraHints: string[] } {
  const verbosity = (query as WithVerbosity<typeof query>).verbosity;
  const nextPathHints = buildNextPathHints(
    (input.data as { structure?: unknown }).structure,
    input.entryCount,
    Boolean(input.summary?.truncated)
  );
  if (isConcise(verbosity)) {
    // Keep concise research-grade: drop the full tree but surface a sample of
    // top folder/file names so the agent has a concrete path to drill into.
    // A bare entry count is a dead-end for repo recon; names give the next move.
    const topEntries = collectTopStructureEntries(
      (input.data as { structure?: unknown }).structure,
      CONCISE_TOP_ENTRIES
    );
    const more =
      input.entryCount > topEntries.length
        ? ` (+${input.entryCount - topEntries.length} more)`
        : '';
    const topHint =
      topEntries.length > 0 ? [`top: ${topEntries.join(', ')}${more}`] : [];
    return {
      data: {
        path: (input.data as { path?: string }).path,
        summary: input.summary,
        entryCount: input.entryCount,
      },
      // `topHint` already samples the same top entries (with the same
      // "(+N more)" suffix) as `nextPathHints`, so emitting both is pure
      // duplication in concise mode — keep only `top:`.
      extraHints: [
        `${input.entryCount} entries${input.summary ? ` (${JSON.stringify(input.summary)})` : ''}`,
        ...topHint,
        ...input.extraHints,
      ],
    };
  }
  if (isCompact(verbosity)) {
    return {
      data: input.data,
      extraHints:
        compactTrimHints(
          [...nextPathHints, ...input.extraHints],
          isAdvisoryViewRepoStructureHint,
          2
        ) ?? [],
    };
  }
  return {
    data: input.data,
    extraHints: [...nextPathHints, ...input.extraHints],
  };
}
