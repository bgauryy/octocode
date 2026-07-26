import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type { z } from 'zod';
import type { NpmPackageQuerySchema } from '@octocodeai/octocode-core/schemas';

type NpmSearchQuery = z.input<typeof NpmPackageQuerySchema>;
import { searchPackage } from '../../utils/package/common.js';
import { isExactPackageName } from '../../utils/package/npm.js';
import { foldKeywords, isPackageNotFoundError } from './queryHelpers.js';
// Re-exported so importers/tests of './execution.js' keep a single entry point.
export { foldKeywords, isPackageNotFoundError } from './queryHelpers.js';
import type {
  NpmSearchAPIResult,
  NpmSearchError,
  PackageResult,
  NpmPackageResult,
} from '../../utils/package/common.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import { createSuccessResult, createErrorResult } from '../utils.js';
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

// npm's shorthand repository hosts (`github:owner/repo`, `gitlab:…`,
// `bitbucket:…`) map to these canonical hosts.
const SHORTHAND_REPO_HOSTS: Record<string, string> = {
  github: 'github.com',
  gitlab: 'gitlab.com',
  bitbucket: 'bitbucket.org',
};

/**
 * Normalize the many repository URL shapes npm packages carry
 * (ssh://git@…, git+https://…, git://…, git@host:owner/repo, and npm's
 * `github:owner/repo` / `gitlab:` / `bitbucket:` shorthands) into a
 * canonical `https://host/owner/repo` form. Falls back to the cleaned
 * original when the shape is unrecognized.
 */
export function normalizeRepoUrl(
  url: string | null | undefined
): string | undefined {
  if (!url) return undefined;
  let u = url.trim();
  if (!u) return undefined;

  // Strip leading VCS prefixes: git+https://, git+ssh://, git+ (etc.)
  u = u.replace(/^git\+/, '');
  // Drop a trailing .git suffix.
  const stripGit = (s: string): string => s.replace(/\.git$/, '');

  // npm shorthand: github:owner/repo, gitlab:owner/repo, bitbucket:owner/repo.
  // Must run before the scheme match — these have no `//` so they'd otherwise
  // fall through to the bare-form branch and never resolve to a GitHub repo.
  const shorthandMatch = u.match(/^(github|gitlab|bitbucket):(.+)$/i);
  if (shorthandMatch && shorthandMatch[1] && shorthandMatch[2]) {
    const host = SHORTHAND_REPO_HOSTS[shorthandMatch[1].toLowerCase()];
    return stripGit(`https://${host}/${shorthandMatch[2]}`);
  }

  // scp-like syntax: git@github.com:owner/repo(.git)
  const scpMatch = u.match(/^[^@/]+@([^:/]+):(.+)$/);
  if (scpMatch && scpMatch[1] && scpMatch[2]) {
    return stripGit(`https://${scpMatch[1]}/${scpMatch[2]}`);
  }

  // ssh://git@host/owner/repo, git://host/owner/repo, https://host/owner/repo
  const schemeMatch = u.match(/^(?:ssh|git|https?):\/\/(.+)$/);
  if (schemeMatch && schemeMatch[1]) {
    // Drop any userinfo (e.g. git@) from the authority component.
    const rest = schemeMatch[1].replace(/^[^@/]+@/, '');
    return stripGit(`https://${rest}`);
  }

  // Bare owner/repo or unknown shape: return cleaned form unchanged.
  return stripGit(u);
}

function resolveGitHubOwnerRepo(
  repoUrl: string | undefined
): { owner: string; repo: string } | undefined {
  if (!repoUrl) return undefined;
  const match = repoUrl.match(/^https:\/\/github\.com\/([^/]+)\/([^/#?]+)/i);
  if (!match || !match[1] || !match[2]) return undefined;
  return { owner: match[1], repo: match[2] };
}

type PackageData = {
  name: string;
  version?: string;
  description?: string;
  license?: string;
  downloads?: number;
  repository?: string;
  repositoryDirectory?: string;
  repositoryId?: string;
  next?: Record<string, unknown>;
  warnings?: string[];
};

type PackageRepositoryData = {
  repository: string;
  owner: string;
  repo: string;
  repositoryDirectory?: string;
  next: Record<string, unknown>;
};

type CompactPackageData = {
  packages: PackageData[];
  repositories?: Record<string, PackageRepositoryData>;
};

function buildNext(
  repoUrl: string | undefined,
  repositoryDirectory: string | undefined
): Record<string, unknown> | undefined {
  const gh = resolveGitHubOwnerRepo(repoUrl);
  if (!gh) return undefined;
  const { owner, repo } = gh;
  const next: Record<string, unknown> = {
    viewRepoStructure: {
      tool: 'ghViewRepoStructure',
      query: {
        owner,
        repo,
        ...(repositoryDirectory ? { path: repositoryDirectory } : {}),
      },
    },
    latestRelease: {
      tool: 'ghListReleases',
      query: { owner, repo },
      why: 'npm latest can diverge from the repo release line — verify against the repo releases/tags (requires ENABLE_RELEASES)',
    },
    searchCode: {
      tool: 'ghSearchCode',
      query: { owner, repo },
    },
    cloneRepo: {
      tool: 'ghCloneRepo',
      query: {
        owner,
        repo,
        ...(repositoryDirectory ? { sparsePath: repositoryDirectory } : {}),
      },
    },
  };
  return next;
}

export function formatPackageData(pkg: PackageResult): PackageData {
  const name = getPackageName(pkg);
  const url = normalizeRepoUrl(getPackageRepo(pkg));
  const data: PackageData = { name };
  if (isNpm(pkg)) {
    if (pkg.version && pkg.version !== 'unknown') data.version = pkg.version;
    if (pkg.description) data.description = pkg.description;
    if (pkg.license) data.license = pkg.license;
    if (typeof pkg.weeklyDownloads === 'number')
      data.downloads = pkg.weeklyDownloads;
  }
  if (url) data.repository = url;
  const root = cleanRelativePath(
    isNpm(pkg) ? pkg.repositoryDirectory : undefined
  );
  if (root) data.repositoryDirectory = root;
  const next = buildNext(url, root);
  if (next) data.next = next;
  // Registry-latest can diverge from the linked repo's release line (e.g. npm
  // typescript@7.x vs microsoft/TypeScript tags at v6.x) — warn so agents
  // don't cite the npm version as the repo's latest release.
  if (data.version && resolveGitHubOwnerRepo(url)) {
    data.warnings = [
      `npm version ${data.version} is the registry's latest publish and may not correspond to a release/tag in the linked repository — verify against the repo's releases or tags before citing it as the repo's latest release.`,
    ];
  }
  return data;
}

export function compactPackageRepositories(
  packages: PackageData[]
): CompactPackageData {
  if (packages.length <= 1) return { packages };

  const repositories: Record<string, PackageRepositoryData> = {};
  const idsByKey = new Map<string, string>();
  let nextId = 1;

  const compacted = packages.map(pkg => {
    if (!pkg.repository) return pkg;
    const gh = resolveGitHubOwnerRepo(pkg.repository);
    if (!gh) return pkg;

    const key = `${pkg.repository}\0${pkg.repositoryDirectory ?? ''}`;
    let id = idsByKey.get(key);
    if (!id) {
      id = `r${nextId++}`;
      idsByKey.set(key, id);
      repositories[id] = {
        repository: pkg.repository,
        owner: gh.owner,
        repo: gh.repo,
        ...(pkg.repositoryDirectory
          ? { repositoryDirectory: pkg.repositoryDirectory }
          : {}),
        next: buildNext(pkg.repository, pkg.repositoryDirectory)!,
      };
    }

    const { repository, repositoryDirectory, next, ...rest } = pkg;
    void repository;
    void repositoryDirectory;
    void next;
    return {
      ...rest,
      repositoryId: id,
    };
  });

  return Object.keys(repositories).length
    ? { packages: compacted, repositories }
    : { packages };
}

type PackagePagination = {
  currentPage: number;
  totalPages: number;
  perPage: number;
  totalFound: number;
  returned: number;
  hasMore: boolean;
  nextPage?: number;
};

export function buildPackagePagination(
  query: NpmSearchQuery,
  totalFound: number,
  returned: number,
  isKeyword: boolean
): PackagePagination {
  const currentPage = Math.max(1, (query as { page?: number }).page ?? 1);
  const perPage = isKeyword ? 10 : 1;

  // The keyword CLI search path reports `totalFound` as the count returned on
  // THIS page (capped at the search limit), not the registry grand total. When
  // a page comes back FULL, that count understates the true total and would
  // otherwise yield hasMore:false even though more pages exist. Treat a full
  // page as "there is at least one more page" so deeper paging stays reachable;
  // only trust `totalFound` for a true totalPages when the page is NOT full.
  const pageIsFull = isKeyword && returned >= perPage;
  const totalPagesFromCount = Math.max(1, Math.ceil(totalFound / perPage));
  const hasMore = pageIsFull || currentPage < totalPagesFromCount;
  // Don't claim a precise totalPages from an understated count when the page is
  // full and the count doesn't already imply a further page.
  const totalPages =
    pageIsFull && currentPage >= totalPagesFromCount
      ? currentPage + 1
      : totalPagesFromCount;

  return {
    currentPage,
    totalPages,
    perPage,
    totalFound,
    returned,
    hasMore,
    ...(hasMore ? { nextPage: currentPage + 1 } : {}),
  };
}

export async function searchPackages(
  args: ToolExecutionArgs<NpmSearchQuery>
): Promise<CallToolResult> {
  return executeBulkOperation(
    args.queries,
    async (query: NpmSearchQuery) => {
      try {
        // Explicit packageName wins; otherwise derive the registry query from
        // `keywords` (folding an array to a space-joined string).
        const searchName =
          query.packageName ??
          foldKeywords((query as { keywords?: string | string[] }).keywords);
        if (!searchName) {
          return createErrorResult(
            'Provide a packageName or keywords for package search',
            query
          );
        }

        const apiResult = await searchPackage({
          name: searchName,
          page: (query as { page?: number }).page,
          mainResearchGoal: (query as { mainResearchGoal?: string })
            .mainResearchGoal,
          researchGoal: (query as { researchGoal?: string }).researchGoal,
          reasoning: (query as { reasoning?: string }).reasoning,
        });

        if (isNpmSearchError(apiResult)) {
          // A genuine not-found (registry has nothing under this name/terms) —
          // whether an exact lookup or a keyword miss — is a guided empty, not
          // a hard error. Only true network/transport failures stay errors.
          if (isPackageNotFoundError(apiResult.error)) {
            const exact = isExactPackageName(searchName);
            return createSuccessResult(
              query,
              {
                packages: [],
                hints: [
                  exact
                    ? `No package published under the exact name "${searchName}". Check the spelling, and try the scoped (@scope/name) vs unscoped form.`
                    : `No packages matched "${searchName}". Try different or fewer keywords, or search by an exact package name.`,
                ],
              },
              false,
              TOOL_NAMES.PACKAGE_SEARCH,
              { rawResponse: apiResult }
            );
          }
          return createErrorResult(apiResult.error, query, {
            rawResponse: apiResult,
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

        const compacted = compactPackageRepositories(packages);
        const data = {
          ...compacted,
          pagination,
        };

        return createSuccessResult(
          query,
          data,
          hasContent,
          TOOL_NAMES.PACKAGE_SEARCH,
          {
            rawResponse: apiResult.rawResponseChars ?? apiResult,
          }
        );
      } catch (error) {
        return createErrorResult(error, query, {
          toolName: TOOL_NAMES.PACKAGE_SEARCH,
        });
      }
    },
    {
      toolName: TOOL_NAMES.PACKAGE_SEARCH,
      keysPriority: ['packages', 'pagination', 'error'],
    },
    args
  );
}
