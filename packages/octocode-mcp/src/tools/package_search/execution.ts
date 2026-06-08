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

function truncateText(
  value: string | undefined,
  maxLength = 200
): string | undefined {
  if (!value) return undefined;
  if (value.length <= maxLength) return value;
  return `${value.slice(0, Math.max(0, maxLength - 3))}...`;
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

type ShapedPackage = Record<string, unknown> & { name: string };

function getPackageField<T>(pkg: PackageResult, field: string): T | undefined {
  return (pkg as unknown as Record<string, unknown>)[field] as T | undefined;
}

function cleanRelativePath(
  path: string | null | undefined
): string | undefined {
  if (!path) return undefined;
  const clean = path.replace(/^\.\//, '').replace(/^\//, '');
  return clean.length > 0 ? clean : undefined;
}

function joinRepoPath(
  root: string | undefined,
  entry: string | null | undefined
): string | undefined {
  const cleanEntry = cleanRelativePath(entry ?? undefined);
  if (!cleanEntry) return undefined;
  const cleanRoot = cleanRelativePath(root);
  return cleanRoot
    ? `${cleanRoot.replace(/\/$/, '')}/${cleanEntry}`
    : cleanEntry;
}

function compactExports(
  exportsList: string[] | undefined
): string[] | undefined {
  if (!exportsList || exportsList.length === 0) return undefined;
  return exportsList.slice(0, 12);
}

function buildPackageType(pkg: PackageResult): string {
  return getPackageField<string>(pkg, 'packageType') ?? 'unknown';
}

function buildEntrypoints(pkg: PackageResult) {
  const main = getPackageField<string | null>(pkg, 'mainEntry') ?? null;
  const module = getPackageField<string | null>(pkg, 'moduleEntry') ?? null;
  const types = getPackageField<string | null>(pkg, 'typeDefinitions') ?? null;
  const exportsList = compactExports(getPackageField<string[]>(pkg, 'exports'));
  if (!main && !module && !types && !exportsList) return undefined;
  return {
    ...(main ? { main } : {}),
    ...(module ? { module } : {}),
    ...(types ? { types } : {}),
    ...(exportsList ? { exports: exportsList } : {}),
  };
}

function buildResearchTargets(
  pkg: PackageResult,
  owner?: string,
  repo?: string
) {
  if (!owner || !repo) return undefined;
  const sourceRoot =
    cleanRelativePath(getPackageField<string>(pkg, 'repositoryDirectory')) ??
    '';
  const name = getPackageName(pkg);
  const entrypoints = buildEntrypoints(pkg);
  const fileContent = [
    {
      path: joinRepoPath(sourceRoot, entrypoints?.main),
      purpose: 'runtime' as const,
    },
    {
      path: joinRepoPath(sourceRoot, entrypoints?.module),
      purpose: 'module' as const,
    },
    {
      path: joinRepoPath(sourceRoot, entrypoints?.types),
      purpose: 'types' as const,
    },
  ]
    .filter(
      (
        entry
      ): entry is {
        path: string;
        purpose: 'runtime' | 'module' | 'types';
      } => Boolean(entry.path)
    )
    .map(entry => ({ owner, repo, ...entry }));

  return {
    repoStructure: { owner, repo, path: sourceRoot },
    codeSearch: {
      owner,
      repo,
      ...(sourceRoot ? { path: sourceRoot } : {}),
      keywordsToSearch: [name],
    },
    ...(fileContent.length > 0 ? { fileContent } : {}),
  };
}

function shapePackage(pkg: PackageResult): ShapedPackage {
  const repoUrl = getPackageRepo(pkg);
  const { owner, repo } = parseRepoInfo(repoUrl);
  const name = getPackageName(pkg);
  const repositoryDirectory = cleanRelativePath(
    getPackageField<string>(pkg, 'repositoryDirectory')
  );
  const description = truncateText(getPackageField<string>(pkg, 'description'));
  const entrypoints = buildEntrypoints(pkg);
  const researchTargets = buildResearchTargets(pkg, owner, repo);
  const {
    path: _path,
    dependencies: _dependencies,
    peerDependencies: _peerDependencies,
    ...pkgRest
  } = pkg as PackageResult & {
    path?: string;
    dependencies?: Record<string, string>;
    peerDependencies?: Record<string, string>;
  };

  return {
    ...pkgRest,
    name,
    ...(description ? { description } : {}),
    packageType: buildPackageType(pkg),
    ...(owner && repo ? { owner, repo } : {}),
    ...(repoUrl
      ? {
          repository: {
            url: repoUrl,
            ...(owner && repo ? { owner, repo } : {}),
            ...(repositoryDirectory ? { directory: repositoryDirectory } : {}),
            sourceRoot: repositoryDirectory ?? '',
          },
        }
      : {}),
    ...(entrypoints ? { entrypoints } : {}),
    ...(researchTargets ? { researchTargets } : {}),
  };
}

export async function searchPackages(
  args: ToolExecutionArgs<PackageSearchQuery>
): Promise<CallToolResult> {
  const { queries } = args;

  return executeBulkOperation(
    queries,
    async (query: PackageSearchQuery, _index: number) => {
      try {
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

        const rawPackages = apiResult.packages as PackageResult[];
        const packages = rawPackages.map(pkg => shapePackage(pkg));

        const result = {
          packages,
          totalFound: apiResult.totalFound,
        };

        const hasContent = result.packages.length > 0;

        let deprecationInfo: DeprecationInfo | null = null;
        if (hasContent && rawPackages[0]) {
          deprecationInfo = await checkNpmDeprecation(
            getPackageName(rawPackages[0])
          );
        }

        const extraHints = hasContent
          ? generateSuccessHints(
              { packages: rawPackages, shapedPackages: packages },
              deprecationInfo
            )
          : generateEmptyHints(validatedQuery);

        const shaped = { data: result, extraHints };
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
    shapedPackages?: ShapedPackage[];
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
  const shaped = result.shapedPackages?.[0];
  const targets = shaped?.researchTargets as
    | {
        repoStructure?: { path?: string };
        fileContent?: Array<{ path: string }>;
      }
    | undefined;
  if (owner && repo) {
    const sourceRoot = targets?.repoStructure?.path ?? '';
    hints.push(
      `Source: github.com/${owner}/${repo}${sourceRoot ? ` (sourceRoot=${sourceRoot})` : ''} — use githubViewRepoStructure or githubSearchCode to explore the implementation.`
    );
    const firstFile = targets?.fileContent?.[0]?.path;
    if (firstFile) {
      hints.push(
        `Entrypoint: use githubGetFileContent owner=${owner} repo=${repo} path=${firstFile}.`
      );
    }
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
