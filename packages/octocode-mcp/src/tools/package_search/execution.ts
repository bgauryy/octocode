import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type { z } from 'zod/v4';
import type { NpmPackageQuerySchema } from '@octocodeai/octocode-core/schemas';

type PackageSearchQuery = Omit<
  z.infer<typeof NpmPackageQuerySchema>,
  'ecosystem'
> & {
  ecosystem?: 'npm';
};
import {
  searchPackage,
  checkNpmDeprecation,
} from '../../utils/package/common.js';
import type {
  PackageSearchAPIResult,
  PackageSearchError,
  PackageResult,
  DeprecationInfo,
} from '../../utils/package/common.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import {
  isConcise,
  isCompact,
  compactTrimHints,
  makeAdvisoryPredicate,
} from '../../scheme/verbosity.js';
import type { WithVerbosity } from '../../scheme/localSchemaOverlay.js';

const CONCISE_PACKAGE_SEARCH_LIMIT = 3;

/** Advisory hints packageSearch emits; stripped under compact. Substring-OR,
 * case-insensitive — tolerates wording shifts and surrounding wrappers. */
const isAdvisoryPackageSearchHint = makeAdvisoryPredicate([
  'searchlimit',
  'scoped package',
  'spelling',
  'alternative',
]);
import {
  handleCatchError,
  createSuccessResult,
  createErrorResult,
} from '../utils.js';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import type { ToolExecutionArgs } from '../../types/execution.js';

function isPackageSearchError(
  result: PackageSearchAPIResult | PackageSearchError
): result is PackageSearchError {
  return 'error' in result;
}

function getPackageName(pkg: PackageResult): string {
  if ('path' in pkg) {
    return pkg.path;
  }
  return pkg.name;
}

function getPackageRepo(pkg: PackageResult): string | null {
  if ('repoUrl' in pkg) {
    return pkg.repoUrl;
  }
  return pkg.repository;
}

function parseRepoInfo(repoUrl: string | null | undefined): {
  owner?: string;
  repo?: string;
} {
  if (!repoUrl) return {};
  const match = repoUrl.match(/github\.com\/([^/]+)\/([^/]+)/);
  if (match && match[1] && match[2]) {
    const owner = match[1];
    const repoName = match[2];
    const cleanRepo = repoName.replace(/\.git$/, '').replace(/\/$/, '');
    return { owner, repo: cleanRepo };
  }
  return {};
}

// Pagination note: the returned `packages` array is NOT a data-loss surface —
// it is char-paginated losslessly by the bulk engine (PACKAGE_SEARCH case in
// structuredPagination.ts → paginatePackageEntry), so every returned package is
// reachable by advancing the charOffset / responseCharOffset cursor. `itemsPerPage`
// is the explicit fetch cap (how many the registry query returns), the cross-tool
// page-size knob that replaced the old upstream `searchLimit`.
// TODO (feature, not data loss): fetching results BEYOND itemsPerPage needs a
// registry result-page cursor, and per-result lastPublished/weeklyDownloads
// enrichment is currently only applied to exact-match lookups (itemsPerPage=1)
// via fetchPackageDetailsWithError.
export async function searchPackages(
  args: ToolExecutionArgs<PackageSearchQuery>
): Promise<CallToolResult> {
  const { queries, responseCharOffset, responseCharLength } = args;

  return executeBulkOperation(
    queries,
    async (query: PackageSearchQuery, _index: number) => {
      try {
        // Pre-flight verbosity caps under concise: cap searchLimit to 1 and
        // force npmFetchMetadata=false (concise's documented lean contract).
        const pkgVerbosityIsConcise = isConcise(
          (query as WithVerbosity<typeof query>).verbosity
        );
        if (pkgVerbosityIsConcise) {
          const userItemsPerPage = (query as { itemsPerPage?: number })
            .itemsPerPage;
          if (
            typeof userItemsPerPage === 'number' &&
            userItemsPerPage > CONCISE_PACKAGE_SEARCH_LIMIT
          ) {
            (query as { itemsPerPage?: number }).itemsPerPage =
              CONCISE_PACKAGE_SEARCH_LIMIT;
          }
          if (
            (query as { npmFetchMetadata?: boolean }).npmFetchMetadata === true
          ) {
            (query as { npmFetchMetadata?: boolean }).npmFetchMetadata = false;
          }
        }

        if (!query.name) {
          return createErrorResult(
            'Package name is required for package search',
            query
          );
        }
        if (query.ecosystem !== undefined && query.ecosystem !== 'npm') {
          return createErrorResult(
            'Only ecosystem="npm" is supported for package search',
            query
          );
        }
        // page > 1 has no registry cursor implementation. Reject early so agents
        // don't silently receive duplicate first-page data. Use `itemsPerPage` to
        // control how many results the first (and only) page returns.
        const requestedPage = (query as { page?: number }).page ?? 1;
        if (requestedPage > 1) {
          return createErrorResult(
            `packageSearch does not support page=${requestedPage}. Only page=1 is implemented. Use itemsPerPage to control result count.`,
            query
          );
        }
        const validatedQuery = {
          ...query,
          ecosystem: query.ecosystem ?? ('npm' as const),
        } as PackageSearchQuery & {
          ecosystem: 'npm';
          name: string;
        };
        const apiResult = await searchPackage(validatedQuery);

        if (isPackageSearchError(apiResult)) {
          return createErrorResult(apiResult.error, query, {
            rawResponse: apiResult,
          });
        }

        const packages = (apiResult.packages as PackageResult[]).map(pkg => {
          const repoUrl = getPackageRepo(pkg);
          const { owner, repo } = parseRepoInfo(repoUrl);
          const name = getPackageName(pkg);
          return { ...pkg, name, ...(owner && repo ? { owner, repo } : {}) };
        });

        const result = {
          packages,
          totalFound: apiResult.totalFound,
        };

        const hasContent = result.packages.length > 0;

        let deprecationInfo: DeprecationInfo | null = null;
        if (hasContent && result.packages[0]) {
          deprecationInfo = await checkNpmDeprecation(
            getPackageName(result.packages[0])
          );
        }

        const extraHints = hasContent
          ? generateSuccessHints(result, deprecationInfo)
          : generateEmptyHints(validatedQuery);

        const shaped = applyPackageSearchVerbosity(
          { data: result, extraHints },
          query
        );
        // `itemsPerPage` is what we asked the registry for. When totalFound is
        // not returned by the API, assume there may be more if the result count
        // exactly hits the requested cap (conservative partial signal).
        const itemsPerPage =
          (query as { itemsPerPage?: number }).itemsPerPage ?? 20;
        const isPartial =
          typeof result.totalFound === 'number'
            ? result.totalFound > result.packages.length
            : result.packages.length >= itemsPerPage;
        const partialReason =
          typeof result.totalFound === 'number'
            ? `${result.packages.length} of ${result.totalFound} package result(s) returned.`
            : `${result.packages.length} result(s) returned; registry did not report total — there may be more. Try a more specific name or reduce itemsPerPage.`;

        return createSuccessResult(
          query,
          shaped.data,
          hasContent,
          TOOL_NAMES.PACKAGE_SEARCH,
          {
            extraHints: shaped.extraHints,
            evidence: {
              kind: 'package',
              answerReady: hasContent,
              complete: !isPartial,
              ...(isPartial
                ? {
                    confidence: 'medium' as const,
                    reason: partialReason,
                  }
                : hasContent
                  ? {}
                  : {
                      reason:
                        'No package registry results matched the supplied query.',
                    }),
            },
            rawResponse: apiResult.rawResponseChars ?? apiResult,
          }
        );
      } catch (error) {
        return handleCatchError(error, query);
      }
    },
    {
      toolName: TOOL_NAMES.PACKAGE_SEARCH,
      keysPriority: ['packages', 'totalFound', 'error'],
      responseCharOffset,
      responseCharLength,
      peerHints: true,
      peerEvidence: true,
    }
  );
}

function generateSuccessHints(
  result: {
    packages: PackageResult[];
  },
  deprecationInfo?: DeprecationInfo | null
): string[] {
  const hints: string[] = [];
  const pkg = result.packages[0];
  if (!pkg) return hints;

  const name = getPackageName(pkg);

  if (deprecationInfo?.deprecated) {
    const msg = deprecationInfo.message || 'This package is deprecated';
    hints.push(`DEPRECATED: ${name} - ${msg}`);
  }

  // Exact install command using the resolved package name — an actionable
  // next step that uses data only available after the registry search resolves.
  hints.push(`Install: npm install ${name}`);

  // Escalation path: guide agents to the GitHub source for deeper research.
  const repoUrl = getPackageRepo(pkg);
  const { owner, repo } = parseRepoInfo(repoUrl);
  if (owner && repo) {
    hints.push(
      `Source: github.com/${owner}/${repo} — use githubViewRepoStructure or githubSearchCode to explore the implementation.`
    );
  } else if (repoUrl) {
    hints.push(
      `Repository: ${repoUrl} — use githubSearchRepositories to find it on GitHub.`
    );
  }

  return hints;
}

function generateEmptyHints(query: PackageSearchQuery): string[] {
  const hints: string[] = [];
  const name = query.name;

  hints.push(`No npm packages found for '${name}'`);

  const variations = generateNameVariations(name);
  if (variations.length > 0) {
    hints.push(`Try: ${variations.join(', ')}`);
  }

  return hints;
}

/**
 * Per-tool verbosity shaping for packageSearch. Under concise, projects each
 * package to {name, version, repository, deprecated} (cap 3) and emits a
 * summary + drill-back hint. Under compact, advisory hints are trimmed to 2.
 * Basic / omitted: passthrough.
 */
export function applyPackageSearchVerbosity(
  input: {
    data: { packages: PackageResult[]; totalFound: number };
    extraHints: string[];
  },
  query: PackageSearchQuery
): {
  data: { packages: unknown[]; totalFound: number };
  extraHints: string[];
} {
  const verbosity = (query as WithVerbosity<typeof query>).verbosity;

  if (isConcise(verbosity)) {
    const projected = (input.data.packages ?? [])
      .slice(0, CONCISE_PACKAGE_SEARCH_LIMIT)
      .map(p => ({
        name: getPackageName(p),
        version: (p as { version?: string }).version,
        repository: getPackageRepo(p),
        deprecated: (p as { deprecated?: unknown }).deprecated,
      }));
    const summary = `${input.data.packages?.length ?? 0} packages found`;
    return {
      data: { packages: projected, totalFound: input.data.totalFound },
      extraHints: [summary, ...input.extraHints],
    };
  }

  const allHints = [...input.extraHints];
  if (isCompact(verbosity)) {
    return {
      data: input.data,
      extraHints:
        compactTrimHints(allHints, isAdvisoryPackageSearchHint, 2) ?? [],
    };
  }
  return { data: input.data, extraHints: allHints };
}

function generateNameVariations(name: string): string[] {
  const variations: string[] = [];

  if (name.includes('-')) {
    variations.push(name.replace(/-/g, '_'));
    variations.push(name.replace(/-/g, ''));
  }
  if (name.includes('_')) {
    variations.push(name.replace(/_/g, '-'));
  }

  if (name.startsWith('@')) {
    const unscoped = name.split('/').pop();
    if (unscoped) variations.push(unscoped);
  }

  if (!name.endsWith('js')) {
    variations.push(name + 'js');
  }

  return [...new Set(variations)].filter(v => v !== name).slice(0, 3);
}
