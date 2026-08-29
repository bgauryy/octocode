/**
 * Hand-authored, per-tool example query patterns for `--scheme`/help output.
 * Split out of `directToolCatalog.meta.ts` (still the public barrel) — see
 * that file's header comment for the full P3 rationale. Kept as its own file
 * because the if-chain of literal example payloads is large but simple, and
 * isolating it keeps `toolCommandPatterns.ts` (the logic that consumes it)
 * small and easy to read.
 */
import {
  LSP_GET_SEMANTICS_TOOL_NAME,
  LOCAL_ANALYZE_GRAPH_TOOL_NAME,
  STATIC_TOOL_NAMES,
} from '../toolNames.js';
import { buildOptionalDirectToolCommandPatternQueries } from './toolCommandPatternOptionalQueries.js';

export function buildKnownDirectToolCommandPatternQueries(
  toolName: string
): Array<{ label: string; query: Record<string, unknown> }> {
  const optionalPatterns =
    buildOptionalDirectToolCommandPatternQueries(toolName);
  if (optionalPatterns.length > 0) return optionalPatterns;

  if (toolName === STATIC_TOOL_NAMES.GITHUB_PULL_REQUESTS) {
    // Split tool: PRs only (no `type` field — commits/issues/releases are their
    // own tools now). List mode uses keywords+filters; detail mode uses prNumber
    // + content selectors.
    return [
      {
        label: 'PR search (list)',
        query: {
          owner: 'bgauryy',
          repo: 'octocode',
          keywordsToSearch: ['localSearchCode'],
          concise: true,
          limit: 5,
        },
      },
      {
        label: 'PR detail (diffs + reviews)',
        query: {
          owner: 'sindresorhus',
          repo: 'slugify',
          prNumber: 1,
          content: {
            comments: { discussion: true },
            reviews: true,
            patches: { mode: 'all' },
          },
        },
      },
    ];
  }

  if (toolName === STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE) {
    return [
      {
        label: 'path search',
        query: {
          keywords: ['package.json'],
          owner: 'bgauryy',
          repo: 'octocode',
          match: 'path',
          concise: true,
          limit: 5,
        },
      },
      {
        label: 'content search',
        query: {
          keywords: ['localSearchCode'],
          owner: 'bgauryy',
          repo: 'octocode',
          extension: 'ts',
          limit: 5,
        },
      },
    ];
  }

  if (toolName === STATIC_TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES) {
    return [
      {
        label: 'repository search',
        query: {
          keywords: ['react'],
          language: 'TypeScript',
          stars: '>1000',
          concise: true,
          limit: 5,
        },
      },
      {
        label: 'owner repositories',
        query: {
          owner: 'bgauryy',
          concise: true,
          limit: 5,
        },
      },
    ];
  }

  if (toolName === STATIC_TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE) {
    return [
      {
        label: 'repo tree',
        query: {
          owner: 'bgauryy',
          repo: 'octocode',
          path: 'packages',
          maxDepth: 2,
          itemsPerPage: 50,
        },
      },
    ];
  }

  if (toolName === STATIC_TOOL_NAMES.GITHUB_CLONE_REPO) {
    return [
      {
        label: 'full repo clone',
        query: {
          owner: 'bgauryy',
          repo: 'octocode',
        },
      },
      {
        label: 'subtree clone',
        query: {
          owner: 'bgauryy',
          repo: 'octocode',
          sparsePath: 'packages/octocode-tools-core',
        },
      },
    ];
  }

  if (toolName === STATIC_TOOL_NAMES.LOCAL_RIPGREP) {
    return [
      {
        label: 'text search',
        query: {
          path: '/ABS/repo/packages/octocode-tools-core/src',
          searchText: 'buildDirectToolCommandPatterns',
          maxFiles: 20,
        },
      },
      {
        label: 'structural code search',
        query: {
          path: '/ABS/repo/packages/octocode-tools-core/src/tools',
          mode: 'structural',
          pattern: 'eval($X)',
        },
      },
    ];
  }

  if (toolName === STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT) {
    return [
      {
        label: 'exact line range',
        query: {
          path: '/ABS/repo/packages/octocode-tools-core/package.json',
          startLine: 1,
          endLine: 30,
          minify: 'none',
        },
      },
      {
        label: 'matched slice',
        query: {
          path: '/ABS/repo/packages/octocode-tools-core/src/tools/directToolCatalog.meta.ts',
          matchString: 'buildKnownDirectToolCommandPatternQueries',
          contextLines: 8,
          minify: 'standard',
        },
      },
    ];
  }

  if (toolName === STATIC_TOOL_NAMES.LOCAL_FIND_FILES) {
    return [
      {
        label: 'basename globs',
        query: {
          path: '/ABS/repo/packages/octocode-tools-core',
          names: ['scheme.ts', 'package.json'],
          entryType: 'f',
          itemsPerPage: 20,
        },
      },
      {
        label: 'monorepo path glob',
        query: {
          path: '/ABS/repo',
          pathPattern: 'packages/*/src/tools/**',
          entryType: 'f',
          itemsPerPage: 20,
        },
      },
      {
        label: 'prune build dirs',
        query: {
          path: '/ABS/repo/packages/octocode-tools-core',
          names: ['*.js'],
          entryType: 'f',
          excludeDir: ['node_modules', 'dist', 'coverage', 'out'],
          itemsPerPage: 20,
        },
      },
    ];
  }

  if (toolName === LOCAL_ANALYZE_GRAPH_TOOL_NAME) {
    return [
      {
        label: 'dead code from detected entrypoints',
        query: {
          operation: 'deadCode',
          path: '/ABS/repo',
          itemsPerPage: 20,
        },
      },
      {
        label: 'cycles',
        query: {
          operation: 'cycles',
          path: '/ABS/repo',
          limit: 20,
        },
      },
      {
        label: 'dependencies of one file',
        query: {
          operation: 'dependencies',
          path: '/ABS/repo',
          file: 'src/index.ts',
          depth: 2,
        },
      },
      {
        label: 'dependents of one file',
        query: {
          operation: 'dependents',
          path: '/ABS/repo',
          file: 'src/index.ts',
          depth: 2,
        },
      },
      {
        label: 'shortest dependency path',
        query: {
          operation: 'path',
          path: '/ABS/repo',
          file: 'src/index.ts',
          target: 'src/responses.ts',
        },
      },
      {
        label: 'reachability from detected entrypoints',
        query: {
          operation: 'reachability',
          path: '/ABS/repo',
          itemsPerPage: 20,
        },
      },
    ];
  }

  if (toolName === STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE) {
    return [
      {
        label: 'shallow tree',
        query: {
          path: '/ABS/repo/packages/octocode-tools-core/src/tools',
          maxDepth: 2,
          itemsPerPage: 50,
        },
      },
      {
        label: 'files only at depth 1',
        query: {
          path: '/ABS/repo/packages/octocode-engine/src',
          maxDepth: 1,
          entryType: 'f',
          itemsPerPage: 100,
        },
      },
    ];
  }

  if (toolName === STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT) {
    return [
      {
        label: 'matched slice (cheap read of one function)',
        query: {
          owner: 'bgauryy',
          repo: 'octocode',
          path: 'packages/octocode-tools-core/src/responses.ts',
          matchString: 'export function cleanJsonObject',
          contextLines: 8,
        },
      },
      {
        label: 'symbols outline (unknown/large file)',
        query: {
          owner: 'bgauryy',
          repo: 'octocode',
          path: 'packages/octocode-tools-core/src/responses.ts',
          minify: 'symbols',
        },
      },
    ];
  }

  if (toolName === STATIC_TOOL_NAMES.GITHUB_COMMITS) {
    return [
      {
        label: 'file history (bounded window)',
        query: {
          owner: 'bgauryy',
          repo: 'octocode',
          path: 'packages/octocode-tools-core/src/responses.ts',
          since: '6m',
          itemsPerPage: 10,
        },
      },
      {
        label: 'compare two refs (base...head)',
        query: {
          owner: 'bgauryy',
          repo: 'octocode',
          base: 'main',
          head: 'update-tools',
        },
      },
    ];
  }

  if (toolName === STATIC_TOOL_NAMES.PACKAGE_SEARCH) {
    return [
      {
        label: 'exact package → source repo',
        query: { packageName: 'zod' },
      },
      {
        label: 'keyword discovery (paged candidates)',
        query: { packageName: 'schema validation', page: 1 },
      },
    ];
  }

  if (toolName === LSP_GET_SEMANTICS_TOOL_NAME) {
    return [
      {
        label: 'symbol outline (absolute uri)',
        query: {
          uri: '/ABS/packages/octocode-tools-core/src/scheme/pagination.ts',
          type: 'documentSymbols',
        },
      },
      {
        label: 'semantic definition (absolute uri + lineHint)',
        query: {
          uri: '/ABS/packages/octocode-tools-core/src/scheme/pagination.ts',
          type: 'definition',
          symbolName: 'buildNextPageContinuation',
          lineHint: 72,
        },
      },
    ];
  }

  return [];
}
