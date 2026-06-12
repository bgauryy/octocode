import { describe, it, expect, beforeEach, vi } from 'vitest';

const { mockMetadata } = vi.hoisted(() => ({
  mockMetadata: {
    instructions: 'Test',
    prompts: {},
    toolNames: {
      GITHUB_FETCH_CONTENT: 'githubGetFileContent',
      GITHUB_SEARCH_CODE: 'githubSearchCode',
      GITHUB_SEARCH_REPOSITORIES: 'githubSearchRepositories',
      LOCAL_RIPGREP: 'localSearchCode',
      LSP_GOTO_DEFINITION: 'lspGotoDefinition',
    },
    baseSchema: {
      mainResearchGoal: 'Main',
      researchGoal: 'Research',
      reasoning: 'Reason',
      bulkQueryTemplate: 'Query {toolName}',
    },
    tools: {
      githubGetFileContent: {
        name: 'githubGetFileContent',
        description: 'Get file content',
        schema: {
          owner: 'Repository owner',
          repo: 'Repository name',
          path: 'File path',
          branch: 'Branch name',
        },
        hints: { hasResults: [], empty: [] },
      },
      githubSearchCode: {
        name: 'githubSearchCode',
        description: 'Search code',
        schema: {
          keywordsToSearch: 'Keywords to search for',
          owner: 'Repo owner',
          repo: 'Repo name',
        },
        hints: { hasResults: [], empty: [] },
      },
      githubSearchRepositories: {
        name: 'githubSearchRepositories',
        description: 'Search repos',
        schema: {
          keywordsToSearch: 'Keywords',
          topicsToSearch: 'Topics',
          stars: 'Star count filter',
        },
        hints: { hasResults: [], empty: [] },
      },
      localSearchCode: {
        name: 'localSearchCode',
        description: 'Local search',
        schema: {
          pattern: 'Search pattern',
          path: 'Search path',
        },
        hints: { hasResults: [], empty: [] },
      },
      lspGotoDefinition: {
        name: 'lspGotoDefinition',
        description: 'Go to definition',
        schema: {
          uri: 'File URI',
          symbolName: 'Symbol name',
          lineHint: 'Line hint',
        },
        hints: { hasResults: [], empty: [] },
      },
    },
    baseHints: { hasResults: [], empty: [] },
    genericErrorHints: [],
  },
}));

vi.mock('@octocodeai/octocode-core', async importOriginal => ({
  ...(await importOriginal<object>()),
  octocodeConfig: mockMetadata,
  completeMetadata: mockMetadata,
}));

describe('toolMetadata/schemaHelpers', () => {
  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
  });

  describe('GITHUB_FETCH_CONTENT', () => {
    it('should access schema fields through proxy', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      const { GITHUB_FETCH_CONTENT } =
        await import('@octocodeai/octocode-core');
      _resetMetadataState();
      await initializeToolMetadata();

      expect(GITHUB_FETCH_CONTENT.scope.owner).toBe('Repo owner');
      expect(GITHUB_FETCH_CONTENT.scope.repo).toBe('Repo name');
      expect(GITHUB_FETCH_CONTENT.scope.path).toBe(
        'File or directory path from root, no leading slash, exact case'
      );
    });

    it('should return empty string for unknown fields', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      const { GITHUB_FETCH_CONTENT } =
        await import('@octocodeai/octocode-core');
      _resetMetadataState();
      await initializeToolMetadata();

      const helper = GITHUB_FETCH_CONTENT as Record<
        string,
        Record<string, string>
      >;
      expect(helper['scope']!['unknown']).toBe('');
    });
  });

  describe('GITHUB_SEARCH_CODE', () => {
    it('should access search schema fields', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      const { GITHUB_SEARCH_CODE } = await import('@octocodeai/octocode-core');
      _resetMetadataState();
      await initializeToolMetadata();

      expect(GITHUB_SEARCH_CODE.search.keywordsToSearch).toBe(
        'Search terms (1-5, AND logic). match=file returns text_matches[]'
      );
      expect(GITHUB_SEARCH_CODE.scope.owner).toBe(
        'Repo owner (omit for cross-repo)'
      );
    });
  });

  describe('GITHUB_SEARCH_REPOS', () => {
    it('should access repo search schema fields', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      const { GITHUB_SEARCH_REPOS } = await import('@octocodeai/octocode-core');
      _resetMetadataState();
      await initializeToolMetadata();

      expect(GITHUB_SEARCH_REPOS.search.topicsToSearch).toBe(
        'GitHub topic tags'
      );
      expect(GITHUB_SEARCH_REPOS.filters.stars).toBe(
        'Stars filter: ">500", "100..500"'
      );
    });
  });

  describe('LOCAL_RIPGREP', () => {
    it('should access local search schema fields', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      const { LOCAL_RIPGREP } = await import('@octocodeai/octocode-core');
      _resetMetadataState();
      await initializeToolMetadata();

      expect(LOCAL_RIPGREP.search.pattern).toBe(
        'Pattern/regex to search (required)'
      );
      expect(LOCAL_RIPGREP.search.path).toBe('Root directory (required)');
    });
  });

  describe('LSP_GOTO_DEFINITION', () => {
    it('should access LSP schema fields', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      const { LSP_GOTO_DEFINITION } = await import('@octocodeai/octocode-core');
      _resetMetadataState();
      await initializeToolMetadata();

      expect(LSP_GOTO_DEFINITION.scope.uri).toBe(
        'File path. Example: "src/utils.ts"'
      );
      expect(LSP_GOTO_DEFINITION.scope.symbolName).toBe(
        'EXACT symbol text. No parens, no partial matches'
      );
      expect(LSP_GOTO_DEFINITION.scope.lineHint).toBe(
        '1-indexed line number. Tool searches +/-2 lines'
      );
    });
  });

  describe('bundled schema helpers', () => {
    it('should read descriptions from octocode-core config (not METADATA_JSON)', async () => {
      const { _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      const { GITHUB_FETCH_CONTENT } =
        await import('@octocodeai/octocode-core');
      _resetMetadataState();

      expect(GITHUB_FETCH_CONTENT.scope.owner).toBe('Repo owner');
    });
  });

  describe('nested proxy access', () => {
    it('should handle any category access', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      const { GITHUB_SEARCH_CODE } = await import('@octocodeai/octocode-core');
      _resetMetadataState();
      await initializeToolMetadata();

      const helper = GITHUB_SEARCH_CODE as Record<
        string,
        Record<string, string>
      >;
      expect(helper['anyCategory']!['anyField']).toBe('');
    });
  });
});
