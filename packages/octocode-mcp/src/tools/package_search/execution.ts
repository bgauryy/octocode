import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type { z } from 'zod/v4';
import type { NpmPackageQuerySchema } from '@octocodeai/octocode-core/schemas';

type NpmPackageQuery = z.infer<typeof NpmPackageQuerySchema>;
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

const CONCISE_PACKAGE_SEARCH_LIMIT = 1;

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

// TODO: packageSearch does not yet support result-level pagination (e.g. lastPublished enrichment
// for search results with searchLimit > 1). Currently only exact-match lookups (searchLimit=1)
// go through fetchPackageDetailsWithError which enriches lastPublished + weeklyDownloads.
export async function searchPackages(
  args: ToolExecutionArgs<NpmPackageQuery>
): Promise<CallToolResult> {
  const { queries, responseCharOffset, responseCharLength, format } = args;

  return executeBulkOperation(
    queries,
    async (query: NpmPackageQuery, _index: number) => {
      try {
        // Pre-flight verbosity caps under concise: cap searchLimit to 1 and
        // force npmFetchMetadata=false (concise's documented lean contract).
        const pkgVerbosityIsConcise = isConcise(
          (query as WithVerbosity<typeof query>).verbosity
        );
        if (pkgVerbosityIsConcise) {
          const userSearchLimit = (query as { searchLimit?: number })
            .searchLimit;
          if (
            typeof userSearchLimit === 'number' &&
            userSearchLimit > CONCISE_PACKAGE_SEARCH_LIMIT
          ) {
            (query as { searchLimit?: number }).searchLimit =
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
        if (query.ecosystem && query.ecosystem !== 'npm') {
          return createErrorResult(
            `Unsupported ecosystem '${query.ecosystem}'. Only 'npm' is supported.`,
            query
          );
        }
        const validatedQuery = {
          ...query,
          ecosystem: 'npm' as const,
        } as NpmPackageQuery & {
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
          : generateEmptyHints(query);

        const shaped = applyPackageSearchVerbosity(
          { data: result, extraHints },
          query
        );

        return createSuccessResult(
          query,
          shaped.data,
          hasContent,
          TOOL_NAMES.PACKAGE_SEARCH,
          {
            extraHints: shaped.extraHints,
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

      format,
      peerHints: true,
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

  return hints;
}

function generateEmptyHints(query: NpmPackageQuery): string[] {
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
 * package to {name, version, repository, deprecated} (cap 1) and emits a
 * summary + drill-back hint. Under compact, advisory hints are trimmed to 2.
 * Basic / omitted: passthrough.
 */
export function applyPackageSearchVerbosity(
  input: {
    data: { packages: PackageResult[]; totalFound: number };
    extraHints: string[];
  },
  query: NpmPackageQuery
): {
  data: { packages: unknown[]; totalFound: number };
  extraHints: string[];
} {
  const verbosity = (query as WithVerbosity<typeof query>).verbosity;

  if (isConcise(verbosity)) {
    const projected = (input.data.packages ?? []).slice(0, 1).map(p => ({
      name: (p as { name?: string }).name,
      version: (p as { version?: string }).version,
      repository: (p as { repository?: string }).repository,
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
