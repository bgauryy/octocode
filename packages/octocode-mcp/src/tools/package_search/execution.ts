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
  NpmPackageResult,
  DeprecationInfo,
} from '../../utils/package/common.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import { createSuccessResult, createErrorResult } from '../utils.js';
import { getHints } from '../../hints/index.js';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import type { ToolExecutionArgs } from '../../types/execution.js';

// ─── helpers ─────────────────────────────────────────────────────────────────

function isPackageSearchError(
  result: PackageSearchAPIResult | PackageSearchError
): result is PackageSearchError {
  return 'error' in result;
}

/** Narrows PackageResult to the richer NpmPackageResult branch (has npmUrl). */
function isNpm(pkg: PackageResult): pkg is NpmPackageResult {
  return 'npmUrl' in pkg;
}

function getPackageName(pkg: PackageResult): string {
  return isNpm(pkg) && pkg.path ? pkg.path : pkg.name;
}

function getPackageRepo(pkg: PackageResult): string | null {
  return isNpm(pkg) ? pkg.repoUrl : pkg.repository;
}

function cleanRelativePath(
  path: string | null | undefined
): string | undefined {
  if (!path) return undefined;
  const clean = path.replace(/^\.\//, '').replace(/^\//, '');
  return clean.length > 0 ? clean : undefined;
}

function parseGitHubRepo(url: string | null | undefined): {
  owner?: string;
  repo?: string;
} {
  if (!url) return {};
  const m = url.match(/github\.com\/([^/]+)\/([^/]+)/);
  if (m?.[1] && m[2]) {
    return { owner: m[1], repo: m[2].replace(/\.git$/, '').replace(/\/$/, '') };
  }
  return {};
}

/**
 * "name url[ sourceRoot]"
 * e.g.
 *   zod https://github.com/colinhacks/zod
 *   react https://github.com/facebook/react packages/react
 */
function formatPackageLine(pkg: PackageResult): string {
  const parts = [getPackageName(pkg)];
  const url = getPackageRepo(pkg);
  if (url) parts.push(url);
  const root = cleanRelativePath(
    isNpm(pkg) ? pkg.repositoryDirectory : undefined
  );
  if (root) parts.push(root);
  return parts.join(' ');
}

// ─── hints ───────────────────────────────────────────────────────────────────

/** Hints for an exact / single-result lookup. */
function exactHints(pkg: PackageResult, dep: DeprecationInfo | null): string[] {
  const hints: string[] = [];
  const name = getPackageName(pkg);

  if (dep?.deprecated)
    hints.push(`DEPRECATED: ${name} — ${dep.message ?? 'use an alternative'}`);

  const src = isNpm(pkg) ? pkg.source : undefined;
  if (src === 'cdn')
    hints.push('Metadata from npm CDN fallback — registry was unavailable.');
  else if (src === 'web')
    hints.push(
      'Metadata from npms.io fallback — verify version when registry access is restored.'
    );

  hints.push(`Install: npm install ${name}`);

  const url = getPackageRepo(pkg);
  const { owner, repo } = parseGitHubRepo(url);
  if (owner && repo)
    hints.push(
      `Browse source: use githubViewRepoStructure owner=${owner} repo=${repo}`
    );
  else if (url)
    hints.push(
      `Repository: ${url} — use githubSearchRepositories to find on GitHub.`
    );
  else
    hints.push(
      `No repository URL for "${name}" — use githubSearchRepositories to find the source repo.`
    );

  return hints;
}

/** Hints for keyword / multi-result searches — no per-package Install/Browse. */
function keywordHints(count: number): string[] {
  return [
    `Found ${count} packages. Re-run with an exact name for source details, install command, and repo navigation.`,
  ];
}

// ─── execution ────────────────────────────────────────────────────────────────

export async function searchPackages(
  args: ToolExecutionArgs<PackageSearchQuery>
): Promise<CallToolResult> {
  return executeBulkOperation(
    args.queries,
    async (query: PackageSearchQuery) => {
      try {
        if (!query.packageName) {
          return createErrorResult(
            'Package name is required for package search',
            query
          );
        }

        const apiResult = await searchPackage({
          name: query.packageName,
          page: (query as { page?: number }).page,
          itemsPerPage: (query as { itemsPerPage?: number }).itemsPerPage,
          mainResearchGoal: (query as { mainResearchGoal?: string })
            .mainResearchGoal,
          researchGoal: (query as { researchGoal?: string }).researchGoal,
          reasoning: (query as { reasoning?: string }).reasoning,
        });

        if (isPackageSearchError(apiResult)) {
          return createErrorResult(apiResult.error, query, {
            rawResponse: apiResult,
            customHints: [
              ...(apiResult.hints ?? []),
              ...getHints(TOOL_NAMES.PACKAGE_SEARCH, 'error', {
                originalError: apiResult.error,
              }),
            ],
          });
        }

        const raw = apiResult.packages as PackageResult[];
        const packages = raw.map(formatPackageLine);
        const hasContent = packages.length > 0;

        // Exact lookup (single result): check deprecation and emit targeted hints.
        // Keyword search (multiple results): skip deprecation, emit generic guidance.
        const isKeyword = raw.length > 1;
        let dep: DeprecationInfo | null = null;
        if (!isKeyword && hasContent && raw[0]) {
          const src = isNpm(raw[0]) ? raw[0].source : undefined;
          if (src !== 'cdn' && src !== 'web') {
            dep = await checkNpmDeprecation(getPackageName(raw[0]));
          }
        }

        const extraHints = !hasContent
          ? getHints(TOOL_NAMES.PACKAGE_SEARCH, 'empty', {
              name: query.packageName,
            } as never)
          : isKeyword
            ? keywordHints(packages.length)
            : exactHints(raw[0]!, dep);

        // Partial when the API returned fewer packages than it knows about.
        const isPartial = packages.length < apiResult.totalFound;
        const data = isPartial
          ? { packages, totalFound: apiResult.totalFound, hasMore: true }
          : { packages, totalFound: apiResult.totalFound };

        return createSuccessResult(
          query,
          data,
          hasContent,
          TOOL_NAMES.PACKAGE_SEARCH,
          {
            extraHints,
            evidence: {
              kind: 'package',
              answerReady: hasContent,
              complete: !isPartial,
              ...(!hasContent
                ? {
                    reason:
                      'No package registry results matched the supplied query.',
                  }
                : {}),
              ...(isPartial
                ? {
                    confidence: 'medium' as const,
                    reason: `${packages.length} of ${apiResult.totalFound} results returned.`,
                  }
                : {}),
            },
            rawResponse: apiResult.rawResponseChars ?? apiResult,
          }
        );
      } catch (error) {
        return createErrorResult(error, query, {
          customHints: getHints(TOOL_NAMES.PACKAGE_SEARCH, 'error', {
            originalError:
              error instanceof Error ? error.message : String(error),
          }),
        });
      }
    },
    {
      toolName: TOOL_NAMES.PACKAGE_SEARCH,
      keysPriority: ['packages', 'totalFound', 'error'],
      peerHints: true,
      peerEvidence: true,
    },
    args
  );
}
