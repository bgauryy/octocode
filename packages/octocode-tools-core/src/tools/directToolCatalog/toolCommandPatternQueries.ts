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
  GITHUB_GET_HISTORY_ITEM_TOOL_NAME,
  GITHUB_SEARCH_HISTORY_TOOL_NAME,
  LOCAL_ANALYZE_GRAPH_TOOL_NAME,
  LOCAL_SEARCH_TOOL_NAME,
  STATIC_TOOL_NAMES,
} from '../toolNames.js';
import { buildOptionalDirectToolCommandPatternQueries } from './toolCommandPatternOptionalQueries.js';
import { buildUnifiedSearchCommandPatternQueries } from './toolCommandPatternUnifiedSearchQueries.js';

export function buildKnownDirectToolCommandPatternQueries(
  toolName: string
): Array<{ label: string; query: Record<string, unknown> }> {
  const optionalPatterns =
    buildOptionalDirectToolCommandPatternQueries(toolName);
  if (optionalPatterns.length > 0) return optionalPatterns;
  const unifiedPatterns = buildUnifiedSearchCommandPatternQueries(toolName);
  if (unifiedPatterns.length > 0) return unifiedPatterns;

  if (toolName === GITHUB_SEARCH_HISTORY_TOOL_NAME) {
    return [
      {
        label: 'PR search (list)',
        query: {
          operation: 'pullRequests',
          owner: 'bgauryy',
          repo: 'octocode',
          keywords: ['localSearch'],
          concise: true,
          pageSize: 5,
        },
      },
      {
        label: 'issue search (list)',
        query: {
          operation: 'issues',
          owner: 'bgauryy',
          repo: 'octocode',
          keywords: ['schema'],
          state: 'open',
          pageSize: 5,
        },
      },
      {
        label: 'commit history',
        query: {
          operation: 'commits',
          owner: 'bgauryy',
          repo: 'octocode',
          path: 'packages/octocode-tools-core/src',
          since: '6m',
          pageSize: 10,
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

  if (toolName === LOCAL_SEARCH_TOOL_NAME) {
    return [
      {
        label: 'text anchors',
        query: {
          operation: 'text',
          path: '/ABS/repo/src',
          searchText: 'buildDirectToolCommandPatterns',
          maxFiles: 20,
        },
      },
      {
        label: 'structural matches',
        query: {
          operation: 'structural',
          path: '/ABS/repo/src',
          pattern: 'eval($X)',
          langType: 'typescript',
        },
      },
      {
        label: 'file discovery',
        query: {
          operation: 'files',
          path: '/ABS/repo',
          names: ['package.json'],
          entryType: 'f',
        },
      },
      {
        label: 'tree orientation',
        query: {
          operation: 'tree',
          path: '/ABS/repo/src',
          maxDepth: 2,
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

  if (toolName === LOCAL_ANALYZE_GRAPH_TOOL_NAME) {
    return [
      {
        label: 'dead code from detected entrypoints',
        query: {
          operation: 'deadCode',
          path: '/ABS/repo',
          limit: 20,
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
          limit: 20,
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

  if (toolName === GITHUB_GET_HISTORY_ITEM_TOOL_NAME) {
    return [
      {
        label: 'pull request detail',
        query: {
          operation: 'pullRequest',
          owner: 'bgauryy',
          repo: 'octocode',
          number: 1,
          content: { body: true, patches: { mode: 'all' } },
        },
      },
      {
        label: 'issue detail',
        query: {
          operation: 'issue',
          owner: 'bgauryy',
          repo: 'octocode',
          number: 1,
          content: { body: true, comments: { discussion: true } },
        },
      },
      {
        label: 'exact commit',
        query: {
          operation: 'commit',
          owner: 'bgauryy',
          repo: 'octocode',
          ref: 'main',
        },
      },
      {
        label: 'compare two refs (base...head)',
        query: {
          operation: 'compare',
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
        query: { keywords: ['schema', 'validation'], page: 1 },
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
