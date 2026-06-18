import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type { z } from 'zod';
import type { NpmPackageQuerySchema } from '@octocodeai/octocode-core/schemas';

type NpmSearchQuery = z.input<typeof NpmPackageQuerySchema>;
import {
  searchPackage,
  checkNpmDeprecation,
} from '../../utils/package/common.js';
import type {
  NpmSearchAPIResult,
  NpmSearchError,
  PackageResult,
  NpmPackageResult,
  DeprecationInfo,
} from '../../utils/package/common.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import { createSuccessResult, createErrorResult } from '../utils.js';
import { getHints } from '../../hints/index.js';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import type { ToolExecutionArgs } from '../../types/execution.js';

function isNpmSearchError(
  result: NpmSearchAPIResult | NpmSearchError
): result is NpmSearchError {
  return 'error' in result;
}

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

type PackageData = {
  name: string;
  version?: string;
  description?: string;
  license?: string;
  weeklyDownloads?: number;
  repository?: string;
  repositoryDirectory?: string;
};

function formatPackageData(pkg: PackageResult): PackageData {
  const name = getPackageName(pkg);
  const url = getPackageRepo(pkg);
  const data: PackageData = { name };
  if (isNpm(pkg)) {
    if (pkg.version && pkg.version !== 'unknown') data.version = pkg.version;
    if (pkg.description) data.description = pkg.description;
    if (pkg.license) data.license = pkg.license;
    if (typeof pkg.weeklyDownloads === 'number')
      data.weeklyDownloads = pkg.weeklyDownloads;
  }
  if (url) data.repository = url;
  const root = cleanRelativePath(
    isNpm(pkg) ? pkg.repositoryDirectory : undefined
  );
  if (root) data.repositoryDirectory = root;
  return data;
}

function exactHints(pkg: PackageResult, dep: DeprecationInfo | null): string[] {
  const hints: string[] = [];
  const name = getPackageName(pkg);

  if (dep?.deprecated)
    hints.push(`DEPRECATED: ${name} — ${dep.message ?? 'use an alternative'}`);

  const src = isNpm(pkg) ? pkg.source : undefined;
  if (src === 'cdn')
    hints.push(
      'Metadata from npm CDN cache — verify version when registry access is restored.'
    );
  else if (src === 'web')
    hints.push(
      'Metadata from npms.io fallback — verify version when registry access is restored.'
    );

  hints.push(`Install: npm install ${name}`);

  const url = getPackageRepo(pkg);
  const { owner, repo } = parseGitHubRepo(url);
  if (owner && repo)
    hints.push(
      `Browse source: use ghViewRepoStructure owner=${owner} repo=${repo}`
    );
  else if (url)
    hints.push(`Repository: ${url} — use ghSearchRepos to find on GitHub.`);
  else
    hints.push(
      `No repository URL for "${name}" — use ghSearchRepos to find the source repo.`
    );

  return hints;
}

type PackagePagination = {
  currentPage: number;
  totalPages: number;
  perPage: number;
  totalFound: number;
  returned: number;
  hasMore: boolean;
};

function buildPackagePagination(
  query: NpmSearchQuery,
  totalFound: number,
  returned: number,
  isKeyword: boolean
): PackagePagination {
  const currentPage = Math.max(1, (query as { page?: number }).page ?? 1);
  const perPage = isKeyword ? 10 : 1;
  const totalPages = Math.max(1, Math.ceil(totalFound / perPage));
  return {
    currentPage,
    totalPages,
    perPage,
    totalFound,
    returned,
    hasMore: currentPage < totalPages,
  };
}

function packagePaginationHints(pagination: PackagePagination): string[] {
  if (pagination.totalFound === 0 || pagination.totalPages <= 1) return [];
  if (pagination.currentPage > pagination.totalPages) {
    return [
      `Requested page ${pagination.currentPage}/${pagination.totalPages} is past the end. Retry page=${pagination.totalPages}.`,
    ];
  }
  const start = (pagination.currentPage - 1) * pagination.perPage + 1;
  const end = Math.min(start + pagination.returned - 1, pagination.totalFound);
  return pagination.hasMore
    ? [
        `Page ${pagination.currentPage}/${pagination.totalPages} (showing ${start}-${end} of ${pagination.totalFound} packages). Next: page=${pagination.currentPage + 1}`,
      ]
    : [];
}

function keywordHints(count: number, totalFound: number): string[] {
  return [
    `Found ${count}${totalFound > count ? ` of ${totalFound}` : ''} packages. Re-run with an exact name for source details, install command, and repo navigation.`,
  ];
}

export async function searchPackages(
  args: ToolExecutionArgs<NpmSearchQuery>
): Promise<CallToolResult> {
  return executeBulkOperation(
    args.queries,
    async (query: NpmSearchQuery) => {
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

        if (isNpmSearchError(apiResult)) {
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
        const packages = raw.map(formatPackageData);
        const hasContent = packages.length > 0;

        const isKeyword = raw.length > 1 || apiResult.totalFound > 1;
        const pagination = buildPackagePagination(
          query,
          apiResult.totalFound,
          packages.length,
          isKeyword
        );
        let dep: DeprecationInfo | null = null;
        if (!isKeyword && hasContent && raw[0]) {
          const src = isNpm(raw[0]) ? raw[0].source : undefined;
          if (src !== 'cdn' && src !== 'web') {
            dep = await checkNpmDeprecation(getPackageName(raw[0]));
          }
        }

        const extraHints = [
          ...packagePaginationHints(pagination),
          ...(!hasContent
            ? getHints(TOOL_NAMES.PACKAGE_SEARCH, 'empty', {
                name: query.packageName,
              } as never)
            : isKeyword
              ? keywordHints(packages.length, apiResult.totalFound)
              : exactHints(raw[0]!, dep)),
        ];

        const data = {
          packages,
          pagination,
        };

        return createSuccessResult(
          query,
          data,
          hasContent,
          TOOL_NAMES.PACKAGE_SEARCH,
          {
            extraHints,
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
      keysPriority: ['packages', 'pagination', 'error'],
      peerHints: true,
    },
    args
  );
}
