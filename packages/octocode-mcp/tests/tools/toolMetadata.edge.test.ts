import { describe, it, expect, vi, beforeEach } from 'vitest';

const hoisted = vi.hoisted(() => {
  const minimalMetadata = {
    toolNames: {
      GITHUB_FETCH_CONTENT: 'githubGetFileContent',
      GITHUB_SEARCH_CODE: 'githubSearchCode',
      GITHUB_SEARCH_REPOSITORIES: 'githubSearchRepositories',
      GITHUB_SEARCH_PULL_REQUESTS: 'githubSearchPullRequests',
      GITHUB_VIEW_REPO_STRUCTURE: 'githubViewRepoStructure',
    },
    baseSchema: {
      mainResearchGoal: 'goal',
      researchGoal: 'goal',
      reasoning: 'reasoning',
      bulkQuery: (_name: string) => 'template',
    },
    tools: {},
    baseHints: { hasResults: [], empty: [] },
    genericErrorHints: [],
    prompts: {},
    instructions: 'test',
  };
  return {
    minimalMetadata,
    octocodeConfig: minimalMetadata,
    octocodeReads: 0,
    completeMetadataReads: 0,
  };
});

vi.mock('@octocodeai/octocode-core', () => ({
  get octocodeConfig() {
    hoisted.octocodeReads++;
    return hoisted.octocodeConfig;
  },
  get completeMetadata() {
    hoisted.completeMetadataReads++;
    return hoisted.octocodeConfig;
  },
}));

describe('toolMetadata - Final Edge Cases', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.resetModules();
    hoisted.octocodeConfig = { ...hoisted.minimalMetadata };
    hoisted.octocodeReads = 0;
    hoisted.completeMetadataReads = 0;
  });

  describe('Concurrent initialization (line 146)', () => {
    it('should handle concurrent initializeToolMetadata calls', async () => {
      const { initializeToolMetadata } =
        await import('../../src/tools/toolMetadata/state.js');

      const promise1 = initializeToolMetadata();
      const promise2 = initializeToolMetadata();
      const promise3 = initializeToolMetadata();

      await Promise.all([promise1, promise2, promise3]);

      expect(hoisted.completeMetadataReads).toBe(1);
    });
  });

  describe('loadToolContent auto-initialization (line 206)', () => {
    it('should auto-initialize when metadata is null', async () => {
      const { loadToolContent } =
        await import('../../src/tools/toolMetadata/state.js');

      const content = await loadToolContent();

      expect(content).toBeDefined();
      expect(content.toolNames).toBeDefined();
    });

    it('should not reinitialize if metadata already loaded', async () => {
      const { initializeToolMetadata, loadToolContent } =
        await import('../../src/tools/toolMetadata/state.js');

      await initializeToolMetadata();
      const readsAfterInit = hoisted.octocodeReads;

      await loadToolContent();
      expect(hoisted.octocodeReads).toBe(readsAfterInit);
    });
  });
});
