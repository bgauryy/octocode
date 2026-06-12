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
import { LOCAL_DEFAULT_FILES_PER_PAGE } from '../../config.js';

type PackageSearchMode = 'smart' | 'full' | 'lean';

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

const EXPORT_CONDITION_RANK = [
  'import',
  'default',
  'require',
  'types',
  'node',
  'browser',
];
function exportConditionRank(condition: string): number {
  const i = EXPORT_CONDITION_RANK.indexOf(condition);
  return i === -1 ? 99 : i;
}

/**
 * Collapse the exports map to one line per subpath. mapExports emits
 * `subpath:condition:target` (object exports) or `subpath:target` (string
 * exports) or a plain path; a package with N subpaths × M conditions would
 * otherwise spill N×M lines (zod ≈ 12). We keep one entry per subpath using
 * the most useful condition (import › default › require › …), rendered as
 * `subpath → target`. No unique subpath is lost; the redundant per-condition
 * duplicates (already covered by the top-level main/types entrypoints) are.
 */
export function compactExports(
  exportsList: string[] | undefined
): string[] | undefined {
  if (!exportsList || exportsList.length === 0) return undefined;

  const bySubpath = new Map<string, { cond: string; target: string }>();
  const plain: string[] = [];

  for (const entry of exportsList) {
    const parts = entry.split(':');
    if (parts.length >= 3) {
      // parts[0] and parts[1] exist — guarded by parts.length >= 3 above.
      const subpath = parts[0]!;
      const condition = parts[1]!;
      const target = parts.slice(2).join(':');
      const current = bySubpath.get(subpath);
      if (
        !current ||
        exportConditionRank(condition) < exportConditionRank(current.cond)
      ) {
        bySubpath.set(subpath, { cond: condition, target });
      }
    } else if (parts.length === 2) {
      // parts[0] and parts[1] exist — guarded by parts.length === 2 above.
      const subpath = parts[0]!;
      if (!bySubpath.has(subpath)) {
        bySubpath.set(subpath, { cond: '', target: parts[1]! });
      }
    } else {
      plain.push(entry);
    }
  }

  const grouped = [...bySubpath.entries()].map(
    ([subpath, { target }]) => `${subpath} → ${target}`
  );
  const out = [...grouped, ...plain].slice(0, 8);
  return out.length > 0 ? out : undefined;
}

function buildPackageType(pkg: PackageResult): string {
  return getPackageField<string>(pkg, 'packageType') ?? 'unknown';
}

function canCheckNpmDeprecation(pkg: PackageResult): boolean {
  const source = getPackageField<string>(pkg, 'source');
  return source !== 'cdn' && source !== 'web';
}

function resolvePackageSearchMode(
  query: PackageSearchQuery
): PackageSearchMode {
  const mode = (query as { mode?: PackageSearchMode }).mode;
  if (mode === 'smart' || mode === 'full' || mode === 'lean') return mode;
  return 'smart';
}

const EXPORT_ARROW = ' → ';
const ROOT_EXPORT_PREFIX = `.${EXPORT_ARROW}`;

function buildEntrypoints(pkg: PackageResult) {
  const rawMain = getPackageField<string | null>(pkg, 'mainEntry') ?? null;
  const module = getPackageField<string | null>(pkg, 'moduleEntry') ?? null;
  const types = getPackageField<string | null>(pkg, 'typeDefinitions') ?? null;
  const exportsAll = compactExports(getPackageField<string[]>(pkg, 'exports'));
  const bin = getPackageField<string[]>(pkg, 'bin');

  // `exports` supersedes `main` (npm spec): when the exports map has a "."
  // entry, that IS the package entry, so we promote it to `main` and list only
  // the OTHER subpaths under `exports` — "." is never shown twice. With no
  // exports map, `main`/`module` stay as the package metadata fallback.
  let main = rawMain;
  let module2: string | null = module;
  let subpathExports: string[] | undefined = exportsAll;
  if (exportsAll) {
    const root = exportsAll.find(e => e.startsWith(ROOT_EXPORT_PREFIX));
    if (root) {
      main = root.slice(ROOT_EXPORT_PREFIX.length);
      module2 = null; // covered by the resolved entry
    }
    const rest = exportsAll.filter(e => !e.startsWith(ROOT_EXPORT_PREFIX));
    subpathExports = rest.length > 0 ? rest : undefined;
  }

  if (!main && !module2 && !types && !subpathExports && !bin) return undefined;
  return {
    ...(main ? { main } : {}),
    ...(module2 ? { module: module2 } : {}),
    ...(types ? { types } : {}),
    ...(bin ? { bin } : {}), // CLI executable location (where the CLI code lives)
    ...(subpathExports ? { exports: subpathExports } : {}),
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
  // Without repository.directory (sourceRoot) the package-relative entrypoint
  // paths cannot be anchored in the repo — on monorepos/gitignored dist they
  // 404 as repo-root paths, so emit fileContent targets only when the
  // monorepo signal exists.
  const fileContent = !sourceRoot
    ? []
    : [
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

/**
 * mode:"lean" is the compact package handoff — keep
 * only the identity + handoff fields (name/version/description/repoUrl/
 * npmUrl/owner/repo) and drop everything research-grade (entrypoints,
 * researchTargets, downloads, license, packageType, lastPublished).
 */
function shapeLeanPackage(pkg: PackageResult): ShapedPackage {
  const repoUrl = getPackageRepo(pkg);
  const { owner, repo } = parseRepoInfo(repoUrl);
  const name = getPackageName(pkg);
  const version = getPackageField<string>(pkg, 'version');
  const description = truncateText(getPackageField<string>(pkg, 'description'));
  const npmUrl = getPackageField<string>(pkg, 'npmUrl');

  return {
    name,
    ...(version ? { version } : {}),
    ...(description ? { description } : {}),
    ...(repoUrl ? { repoUrl } : {}),
    ...(npmUrl ? { npmUrl } : {}),
    ...(owner && repo ? { owner, repo } : {}),
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
    // All entry indicators (main/module/types/exports/bin) are surfaced inside
    // `entrypoints`; drop the raw top-level copies so they aren't duplicated.
    exports: _exports,
    bin: _bin,
    mainEntry: _mainEntry,
    moduleEntry: _moduleEntry,
    typeDefinitions: _typeDefinitions,
    ...pkgRest
  } = pkg as PackageResult & {
    path?: string;
    dependencies?: Record<string, string>;
    peerDependencies?: Record<string, string>;
    exports?: unknown;
    bin?: unknown;
    mainEntry?: unknown;
    moduleEntry?: unknown;
    typeDefinitions?: unknown;
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
        if (!query.packageName) {
          return createErrorResult(
            'Package name is required for package search',
            query
          );
        }
        const mode = resolvePackageSearchMode(query);
        const validatedQuery = {
          ...query,
          mode,
        } as PackageSearchQuery & {
          packageName: string;
        };
        const apiResult = await searchPackage({
          ...validatedQuery,
          name: validatedQuery.packageName,
        });

        if (isPackageSearchError(apiResult)) {
          const errorHints = getHints(TOOL_NAMES.PACKAGE_SEARCH, 'error', {
            originalError: apiResult.error,
          });
          const nameVariations = generateNameVariations(
            validatedQuery.packageName
          );
          const variationHint =
            nameVariations.length > 0
              ? [`Try: ${nameVariations.join(', ')}`]
              : [];
          const mergedHints = [
            ...(apiResult.hints ?? []),
            ...errorHints,
            ...variationHint,
          ];
          return createErrorResult(apiResult.error, query, {
            rawResponse: apiResult,
            customHints: mergedHints,
          });
        }

        const rawPackages = apiResult.packages as PackageResult[];
        const packages = rawPackages.map(pkg =>
          mode === 'lean' ? shapeLeanPackage(pkg) : shapePackage(pkg)
        );

        const result = {
          packages,
          totalFound: apiResult.totalFound,
        };

        const hasContent = result.packages.length > 0;

        let deprecationInfo: DeprecationInfo | null = null;
        if (
          hasContent &&
          rawPackages[0] &&
          canCheckNpmDeprecation(rawPackages[0])
        ) {
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

        const itemsPerPage =
          (query as { itemsPerPage?: number }).itemsPerPage ??
          LOCAL_DEFAULT_FILES_PER_PAGE;
        const currentPage = (query as { page?: number }).page ?? 1;
        const totalPages =
          typeof result.totalFound === 'number'
            ? Math.max(1, Math.ceil(result.totalFound / itemsPerPage))
            : undefined;
        const isPartial =
          typeof result.totalFound === 'number'
            ? currentPage < (totalPages ?? 1)
            : result.packages.length >= itemsPerPage;
        const partialReason =
          typeof result.totalFound === 'number'
            ? `${result.packages.length} of ${result.totalFound} package result(s) returned.`
            : `${result.packages.length} result(s) returned; registry did not report total — there may be more. Try a more specific name or reduce itemsPerPage.`;

        const pagination =
          isPartial && typeof result.totalFound === 'number'
            ? {
                hasMore: true,
                totalFound: result.totalFound,
                returnedCount: result.packages.length,
                currentPage,
                totalPages,
              }
            : undefined;

        const shaped = {
          data: pagination ? { ...result, pagination } : result,
          extraHints,
        };

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
    },
    args
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
  const source = getPackageField<string>(pkg, 'source');

  if (deprecationInfo?.deprecated) {
    const msg = deprecationInfo.message || 'This package is deprecated';
    hints.push(`DEPRECATED: ${name} - ${msg}`);
  }

  if (source === 'cdn') {
    hints.push(
      'Package metadata came from npm CDN package.json fallback because the npm registry was unavailable.'
    );
  } else if (source === 'web') {
    hints.push(
      'Package metadata came from npms.io web search fallback; verify the version with npm when registry access is restored.'
    );
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
    // Built-output entrypoints (dist/build/out bundles) misdirect research —
    // the implementation lives in source, reached via structure/search above.
    const isBuildOutput =
      firstFile !== undefined &&
      /^(dist|build|out|umd|cjs|esm|\.next)\//.test(firstFile);
    if (firstFile && !isBuildOutput) {
      hints.push(
        `Entrypoint: use githubGetFileContent owner=${owner} repo=${repo} path=${firstFile} — verify with githubViewRepoStructure if not found.`
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
  const name = query.packageName;

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
