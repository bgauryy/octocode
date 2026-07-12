import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import path from 'node:path';

import { executeDirectTool } from '../../src/tools/directToolCatalog.js';
import { executeBulkOperation } from '../../src/utils/response/bulk.js';
import { buildGhSearchCodeFinalizer } from '../../src/tools/github_search_code/finalizer.js';
import { buildGithubFetchContentFinalizer } from '../../src/tools/github_fetch_content/finalizer.js';
import type {
  FlatQueryResult,
  ProcessedBulkResult,
} from '../../src/types/toolResults.js';
import { cleanup } from '../../src/serverConfig.js';
import { setRuntimeSurface, _resetRuntimeSurface } from '@octocodeai/config';

// Output schemas (the contract the MCP server advertises + the SDK validates).
import { GitHubCodeSearchOutputLocalSchema } from '../../src/tools/github_search_code/scheme.js';
import { GitHubFetchContentOutputLocalSchema } from '../../src/tools/github_fetch_content/scheme.js';
import { GitHubSearchRepositoriesOutputLocalSchema } from '../../src/tools/github_search_repos/scheme.js';
import { GitHubSearchPullRequestsOutputLocalSchema } from '../../src/tools/github_search_pull_requests/scheme.js';
import { GitHubViewRepoStructureOutputLocalSchema } from '../../src/tools/github_view_repo_structure/scheme.js';
import { GitHubCloneRepoOutputLocalSchema } from '../../src/tools/github_clone_repo/scheme.js';
import { NpmSearchOutputLocalSchema } from '../../src/tools/package_search/scheme.js';
import { LocalSearchCodeOutputSchema } from '../../src/tools/local_ripgrep/scheme.js';
import { LocalFindFilesOutputSchema } from '../../src/tools/local_find_files/scheme.js';
import { LocalViewStructureOutputSchema } from '../../src/tools/local_view_structure/scheme.js';
import { LocalGetFileContentOutputSchema } from '../../src/tools/local_fetch_content/scheme.js';
import { LspGetSemanticsOutputSchema } from '../../src/tools/lsp/semantic_content/scheme.js';

// Local-tool queries resolve against real files under the package dir.
const PKG_DIR = process.cwd();

type StructuredContent = Record<string, unknown>;

/**
 * The SDK validates structuredContent against outputSchema on every NON-error
 * result (isError:true is exempt). A drifted schema turns a good result into a
 * runtime error, so these tests assert the emitted structuredContent parses.
 */
function structuredOf(result: { structuredContent?: unknown }): StructuredContent {
  expect(result.structuredContent).toBeTypeOf('object');
  return result.structuredContent as StructuredContent;
}

describe('MCP outputSchema contract — emitted structuredContent parses', () => {
  const originalEnableLocal = process.env.ENABLE_LOCAL;

  beforeAll(() => {
    setRuntimeSurface('mcp');
    process.env.ENABLE_LOCAL = 'true';
    cleanup();
  });

  afterAll(() => {
    if (originalEnableLocal === undefined) delete process.env.ENABLE_LOCAL;
    else process.env.ENABLE_LOCAL = originalEnableLocal;
    _resetRuntimeSurface();
    cleanup();
  });

  // ----------------------------------------------------------------------
  // Local tools — run the exact MCP path (executeDirectTool) against the real
  // repo, so the structuredContent is genuinely what the server would emit.
  // ----------------------------------------------------------------------

  it('localSearchCode', async () => {
    const result = await executeDirectTool('localSearchCode', {
      queries: [
        {
          path: path.join(PKG_DIR, 'src', 'utils', 'response'),
          keywords: 'sanitizeStructuredContent',
          maxFiles: 5,
          mainResearchGoal: 'contract test',
          researchGoal: 'contract test',
          reasoning: 'contract test',
        },
      ],
    });
    expect(LocalSearchCodeOutputSchema.parse(structuredOf(result))).toBeDefined();
  });

  it('localFindFiles', async () => {
    const result = await executeDirectTool('localFindFiles', {
      queries: [
        {
          path: path.join(PKG_DIR, 'src', 'utils', 'response'),
          namePattern: '*.ts',
          maxDepth: 2,
          mainResearchGoal: 'contract test',
          researchGoal: 'contract test',
          reasoning: 'contract test',
        },
      ],
    });
    expect(LocalFindFilesOutputSchema.parse(structuredOf(result))).toBeDefined();
  });

  it('localViewStructure', async () => {
    const result = await executeDirectTool('localViewStructure', {
      queries: [
        {
          path: path.join(PKG_DIR, 'src'),
          maxDepth: 1,
          mainResearchGoal: 'contract test',
          researchGoal: 'contract test',
          reasoning: 'contract test',
        },
      ],
    });
    expect(
      LocalViewStructureOutputSchema.parse(structuredOf(result))
    ).toBeDefined();
  });

  it('localGetFileContent', async () => {
    const result = await executeDirectTool('localGetFileContent', {
      queries: [
        {
          path: path.join(PKG_DIR, 'package.json'),
          mainResearchGoal: 'contract test',
          researchGoal: 'contract test',
          reasoning: 'contract test',
        },
      ],
    });
    expect(
      LocalGetFileContentOutputSchema.parse(structuredOf(result))
    ).toBeDefined();
  });

  it('lspGetSemantics — resolved definition', async () => {
    const result = await executeDirectTool('lspGetSemantics', {
      queries: [
        {
          uri: path.join(PKG_DIR, 'src', 'utils', 'response', 'bulk.ts'),
          symbolName: 'executeBulkOperation',
          mode: 'definition',
          mainResearchGoal: 'contract test',
          researchGoal: 'contract test',
          reasoning: 'contract test',
        },
      ],
    });
    expect(LspGetSemanticsOutputSchema.parse(structuredOf(result))).toBeDefined();
  });

  it('lspGetSemantics — symbolNotFound (lsp field omitted)', async () => {
    const result = await executeDirectTool('lspGetSemantics', {
      queries: [
        {
          uri: path.join(PKG_DIR, 'src', 'utils', 'response', 'bulk.ts'),
          symbolName: 'noSuchSymbolAnywhere1234',
          mode: 'definition',
          mainResearchGoal: 'contract test',
          researchGoal: 'contract test',
          reasoning: 'contract test',
        },
      ],
    });
    expect(LspGetSemanticsOutputSchema.parse(structuredOf(result))).toBeDefined();
  });

  // ----------------------------------------------------------------------
  // GitHub code search / fetch content — drive the real finalizers (which run
  // cleanJsonObject -> sanitize via formatFinalizedResponse) with fixtures
  // shaped like real provider output. No network.
  // ----------------------------------------------------------------------

  it('ghSearchCode', () => {
    const finalize = buildGhSearchCodeFinalizer();
    const out = finalize({
      queries: [{ id: 'q1', keywords: ['foo'] }] as never,
      results: [
        {
          id: 'q1',
          status: 'success',
          data: {
            results: [
              {
                id: 'octo/repo',
                owner: 'octo',
                repo: 'repo',
                matches: [{ path: 'src/a.ts', value: 'foo bar' }],
              },
            ],
            pagination: {
              currentPage: 1,
              totalPages: 1,
              hasMore: false,
            },
          },
        },
      ] as unknown as FlatQueryResult[],
      config: {} as never,
    });
    expect(
      GitHubCodeSearchOutputLocalSchema.parse(structuredOf(out))
    ).toBeDefined();
  });

  it('ghSearchCode — empty + error rows', () => {
    const finalize = buildGhSearchCodeFinalizer();
    const out = finalize({
      queries: [{ id: 'q1' }, { id: 'q2' }] as never,
      results: [
        {
          id: 'q1',
          status: 'success',
          data: { results: [], incompleteResults: true },
        },
        { id: 'q2', status: 'error', data: { error: 'boom' } },
      ] as unknown as FlatQueryResult[],
      config: {} as never,
    });
    expect(
      GitHubCodeSearchOutputLocalSchema.parse(structuredOf(out))
    ).toBeDefined();
  });

  it('ghGetFileContent', () => {
    const finalize = buildGithubFetchContentFinalizer();
    const out = finalize({
      queries: [
        { id: 'q1', owner: 'octo', repo: 'repo', path: 'README.md' },
      ] as never,
      results: [
        {
          id: 'q1',
          status: 'success',
          data: {
            path: 'README.md',
            content: '# Hello',
            totalLines: 1,
            startLine: 1,
            endLine: 1,
          },
        },
      ] as unknown as FlatQueryResult[],
      config: {} as never,
    });
    expect(
      GitHubFetchContentOutputLocalSchema.parse(structuredOf(out))
    ).toBeDefined();
  });

  it('ghGetFileContent — error row', () => {
    const finalize = buildGithubFetchContentFinalizer();
    const out = finalize({
      queries: [
        { id: 'q1', owner: 'octo', repo: 'repo', path: 'missing.md' },
      ] as never,
      results: [
        { id: 'q1', status: 'error', data: { error: 'not found' } },
      ] as unknown as FlatQueryResult[],
      config: {} as never,
    });
    // Error-only finalizer output is isError; the SDK exempts it, but the
    // structuredContent should still parse.
    expect(
      GitHubFetchContentOutputLocalSchema.parse(structuredOf(out))
    ).toBeDefined();
  });

  // ----------------------------------------------------------------------
  // GitHub repos / PRs / structure / clone and npm — these use the generic
  // (no-finalize) bulk path. Driving executeBulkOperation with a fixture
  // processor exercises the identical envelope + cleanJson -> sanitize path.
  // ----------------------------------------------------------------------

  async function runBulk(
    toolName: string,
    keysPriority: string[],
    data: Record<string, unknown>
  ) {
    const processor = async (): Promise<ProcessedBulkResult> => ({
      status: undefined,
      ...data,
    });
    return executeBulkOperation(
      [{ id: 'q1' }],
      processor,
      { toolName, keysPriority },
    );
  }

  it('ghSearchRepos', async () => {
    const result = await runBulk('ghSearchRepos', ['repositories', 'pagination', 'error'], {
      repositories: [
        {
          owner: 'octo',
          repo: 'repo',
          full_name: 'octo/repo',
          url: 'https://github.com/octo/repo',
          stars: 10,
          language: 'TypeScript',
          description: 'a repo',
        },
      ],
      pagination: { currentPage: 1, totalPages: 1, hasMore: false },
    });
    expect(
      GitHubSearchRepositoriesOutputLocalSchema.parse(structuredOf(result))
    ).toBeDefined();
  });

  it('ghHistoryResearch (pull requests)', async () => {
    const result = await runBulk('ghHistoryResearch', ['pull_requests', 'pagination', 'error'], {
      pull_requests: [
        {
          number: 1,
          title: 'a PR',
          url: 'https://github.com/octo/repo/pull/1',
          state: 'closed',
          merged: true,
          author: 'octo',
        },
      ],
      total_count: 1,
      pagination: { currentPage: 1, totalPages: 1, hasMore: false },
    });
    expect(
      GitHubSearchPullRequestsOutputLocalSchema.parse(structuredOf(result))
    ).toBeDefined();
  });

  it('ghViewRepoStructure', async () => {
    const result = await runBulk('ghViewRepoStructure', ['entries', 'pagination', 'error'], {
      entries: [
        { path: 'src', name: 'src', type: 'dir' },
        { path: 'README.md', name: 'README.md', type: 'file', size: 100 },
      ],
      pagination: { currentPage: 1, totalPages: 1, hasMore: false },
    });
    expect(
      GitHubViewRepoStructureOutputLocalSchema.parse(structuredOf(result))
    ).toBeDefined();
  });

  it('ghCloneRepo', async () => {
    const result = await runBulk('ghCloneRepo', ['localPath', 'resolvedBranch', 'error'], {
      localPath: '/tmp/repo',
      resolvedBranch: 'main',
      cached: false,
      location: {
        kind: 'repo',
        localPath: '/tmp/repo',
        source: 'clone',
      },
    });
    expect(
      GitHubCloneRepoOutputLocalSchema.parse(structuredOf(result))
    ).toBeDefined();
  });

  it('npmSearch', async () => {
    const result = await runBulk('npmSearch', ['packages', 'pagination', 'error'], {
      packages: [
        {
          name: 'react',
          version: '18.0.0',
          license: 'MIT',
          description: 'A library',
          downloads: 1000,
        },
      ],
    });
    expect(NpmSearchOutputLocalSchema.parse(structuredOf(result))).toBeDefined();
  });
});
