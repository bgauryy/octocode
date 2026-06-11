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
      id: 'id',
      mainResearchGoal: 'goal',
      researchGoal: 'goal',
      reasoning: 'reasoning',
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

  describe('loadToolContent', () => {
    it('returns core metadata directly', async () => {
      const { loadToolContent } =
        await import('../../src/tools/toolMetadata/state.js');

      const content = await loadToolContent();

      expect(content).toBeDefined();
      expect(content.toolNames).toBeDefined();
      expect(content).toBe(hoisted.octocodeConfig);
    });

    it('is stable across repeated calls', async () => {
      const { loadToolContent } =
        await import('../../src/tools/toolMetadata/state.js');

      const first = await loadToolContent();
      const second = await loadToolContent();
      expect(second).toBe(first);
    });
  });
});
