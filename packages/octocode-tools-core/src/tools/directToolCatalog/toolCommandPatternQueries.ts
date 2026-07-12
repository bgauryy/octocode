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
  OQL_SEARCH_TOOL_NAME,
  STATIC_TOOL_NAMES,
} from '../toolNames.js';

export function buildKnownDirectToolCommandPatternQueries(
  toolName: string
): Array<{ label: string; query: Record<string, unknown> }> {
  if (toolName === OQL_SEARCH_TOOL_NAME) {
    return [
      {
        label: 'local code query',
        query: {
          schema: 'oql',
          target: 'code',
          from: { kind: 'local', path: '.' },
          where: { kind: 'text', value: 'executeDirectTool' },
          view: 'discovery',
          limit: 5,
        },
      },
    ];
  }

  if (toolName === STATIC_TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS) {
    return [
      {
        label: 'PR search',
        query: {
          type: 'prs',
          owner: 'bgauryy',
          repo: 'octocode',
          keywordsToSearch: ['localSearchCode'],
          concise: true,
          limit: 5,
        },
      },
      {
        label: 'commit history',
        query: {
          type: 'commits',
          owner: 'bgauryy',
          repo: 'octocode',
          path: 'packages/octocode-tools-core/src',
          since: '2024-01-01T00:00:00Z',
          perPage: 5,
        },
      },
      {
        label: 'releases + latest',
        query: {
          type: 'releases',
          owner: 'microsoft',
          repo: 'TypeScript',
          perPage: 5,
        },
      },
      {
        label: 'issues search',
        query: {
          type: 'issues',
          owner: 'microsoft',
          repo: 'TypeScript',
          keywordsToSearch: ['crash'],
          state: 'open',
          concise: true,
          limit: 5,
        },
      },
      {
        label: 'issue detail',
        query: {
          type: 'issues',
          owner: 'bgauryy',
          repo: 'octocode',
          issueNumber: 443,
          content: { body: true, comments: { discussion: true } },
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
          path: 'packages/octocode-tools-core/src',
          keywords: 'buildDirectToolCommandPatterns',
          maxFiles: 20,
        },
      },
      {
        label: 'structural code search',
        query: {
          path: 'packages/octocode-tools-core/src/tools',
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
          path: 'packages/octocode-tools-core/package.json',
          startLine: 1,
          endLine: 30,
          minify: 'none',
        },
      },
      {
        label: 'matched slice',
        query: {
          path: 'packages/octocode-tools-core/src/tools/directToolCatalog.meta.ts',
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
          path: 'packages/octocode-tools-core',
          names: ['scheme.ts', 'package.json'],
          entryType: 'f',
          itemsPerPage: 20,
        },
      },
      {
        label: 'monorepo path glob',
        query: {
          path: '.',
          pathPattern: 'packages/*/src/tools/**',
          entryType: 'f',
          itemsPerPage: 20,
        },
      },
      {
        label: 'prune build dirs',
        query: {
          path: 'packages/octocode-tools-core',
          names: ['*.js'],
          entryType: 'f',
          excludeDir: ['node_modules', 'dist', 'coverage', 'out'],
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
          path: 'packages/octocode-tools-core/src/tools',
          maxDepth: 2,
          itemsPerPage: 50,
        },
      },
      {
        label: 'files only at depth 1',
        query: {
          path: 'packages/octocode-engine/src',
          maxDepth: 1,
          filesOnly: true,
          itemsPerPage: 100,
        },
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
