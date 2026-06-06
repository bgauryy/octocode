import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type { z } from 'zod';
import type { NpmPackageQuerySchema } from '@octocodeai/octocode-core/schemas';

type PackageSearchQuery = Omit<
  z.infer<typeof NpmPackageQuerySchema>,
  'ecosystem'
>;
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
import { isVerbose } from '../../scheme/verbosity.js';
import type { WithVerbosity } from '../../scheme/localSchemaOverlay.js';
import { createSuccessResult, createErrorResult } from '../utils.js';
import { getHints } from '../../hints/index.js';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import type { ToolExecutionArgs } from '../../types/execution.js';

function isPackageSearchError(
  result: PackageSearchAPIResult | PackageSearchError
): result is PackageSearchError {
  return 'error' in result;
}

function getPackageName(pkg: PackageResult): string {
  if ('path' in pkg && typeof pkg.path === 'string') {
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

export async function searchPackages(
  args: ToolExecutionArgs<PackageSearchQuery>
): Promise<CallToolResult> {
  const { queries } = args;

  return executeBulkOperation(
    queries,
    async (query: PackageSearchQuery, _index: number) => {
      try {
        const queryWithVerbosity = query as WithVerbosity<typeof query>;
        if (
          queryWithVerbosity.verbose !== undefined &&
          (query as { npmFetchMetadata?: boolean }).npmFetchMetadata ===
            undefined
        ) {
          (query as { npmFetchMetadata?: boolean }).npmFetchMetadata =
            queryWithVerbosity.verbose;
        }

        if (!query.name) {
          return createErrorResult(
            'Package name is required for package search',
            query
          );
        }
        const validatedQuery = {
          ...query,
        } as PackageSearchQuery & {
          name: string;
        };
        const apiResult = await searchPackage(validatedQuery);

        if (isPackageSearchError(apiResult)) {
          const errorHints = getHints(TOOL_NAMES.PACKAGE_SEARCH, 'error', {
            originalError: apiResult.error,
          });
          const mergedHints = [...(apiResult.hints ?? []), ...errorHints];
          return createErrorResult(apiResult.error, query, {
            rawResponse: apiResult,
            customHints: mergedHints,
          });
        }

        const packages = (apiResult.packages as PackageResult[]).map(pkg => {
          const repoUrl = getPackageRepo(pkg);
          const { owner, repo } = parseRepoInfo(repoUrl);
          const name = getPackageName(pkg);
          const { path: _path, ...pkgRest } = pkg as PackageResult & {
            path?: string;
          };
          return {
            ...pkgRest,
            name,
            ...(owner && repo ? { owner, repo } : {}),
          };
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
        const errorMsg = error instanceof Error ? error.message : String(error);
        const errorHints = getHints(TOOL_NAMES.PACKAGE_SEARCH, 'error', {
          originalError: errorMsg,
        });
        return createErrorResult(error, query, { customHints: errorHints });
      }
    },
    {
      toolName: TOOL_NAMES.PACKAGE_SEARCH,
      keysPriority: ['packages', 'totalFound', 'error'],
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

  hints.push(`Install: npm install ${name}`);

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
  } else {
    hints.push(
      `No repository URL in npm manifest for "${name}" — use githubSearchRepositories with the package name to find the source repo.`
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
  const queryWithVerbosity = query as WithVerbosity<typeof query>;

  if (isVerbose(queryWithVerbosity)) {
    return { data: input.data, extraHints: input.extraHints };
  }

  const METADATA_KEYS = new Set([
    'license',
    'weeklyDownloads',
    'recentVersions',
    'publishedAt',
    'maintainers',
  ]);
  const packages = (input.data.packages ?? []).map(p => {
    const result: Record<string, unknown> = {};
    for (const [key, val] of Object.entries(
      p as unknown as Record<string, unknown>
    )) {
      if (!METADATA_KEYS.has(key)) result[key] = val;
    }
    return result;
  });
  return { data: { ...input.data, packages }, extraHints: input.extraHints };
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
