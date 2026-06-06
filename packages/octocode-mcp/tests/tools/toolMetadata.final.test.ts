import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('@octocodeai/octocode-core', () => ({
  octocodeConfig: {
    instructions: '',
    prompts: {},
    toolNames: {
      GITHUB_FETCH_CONTENT: 'githubGetFileContent',
      GITHUB_SEARCH_CODE: 'githubSearchCode',
      GITHUB_SEARCH_REPOSITORIES: 'githubSearchRepositories',
      GITHUB_SEARCH_PULL_REQUESTS: 'githubSearchPullRequests',
      GITHUB_VIEW_REPO_STRUCTURE: 'githubViewRepoStructure',
    },
    baseSchema: {
      mainResearchGoal: '',
      researchGoal: '',
      reasoning: '',
      bulkQuery: (_: string) => '',
    },
    tools: {},
    baseHints: { hasResults: [], empty: [] },
    genericErrorHints: [],
  },
  completeMetadata: {
    instructions: '',
    prompts: {},
    toolNames: {
      GITHUB_FETCH_CONTENT: 'githubGetFileContent',
      GITHUB_SEARCH_CODE: 'githubSearchCode',
      GITHUB_SEARCH_REPOSITORIES: 'githubSearchRepositories',
      GITHUB_SEARCH_PULL_REQUESTS: 'githubSearchPullRequests',
      GITHUB_VIEW_REPO_STRUCTURE: 'githubViewRepoStructure',
    },
    baseSchema: {
      mainResearchGoal: '',
      researchGoal: '',
      reasoning: '',
      bulkQuery: (_: string) => '',
    },
    tools: {},
    baseHints: { hasResults: [], empty: [] },
    genericErrorHints: [],
  },
}));

describe('toolMetadata - TOOL_NAMES static fallback (lines 236-243)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.resetModules();
  });

  it('should use STATIC_TOOL_NAMES when metadata not loaded', async () => {
    vi.resetModules();

    const { TOOL_NAMES } =
      await import('../../src/tools/toolMetadata/proxies.js');

    const desc1 = Object.getOwnPropertyDescriptor(
      TOOL_NAMES,
      'GITHUB_FETCH_CONTENT'
    );
    const desc2 = Object.getOwnPropertyDescriptor(
      TOOL_NAMES,
      'GITHUB_SEARCH_CODE'
    );
    const desc3 = Object.getOwnPropertyDescriptor(
      TOOL_NAMES,
      'GITHUB_SEARCH_REPOSITORIES'
    );
    const desc4 = Object.getOwnPropertyDescriptor(
      TOOL_NAMES,
      'GITHUB_SEARCH_PULL_REQUESTS'
    );
    const desc5 = Object.getOwnPropertyDescriptor(
      TOOL_NAMES,
      'GITHUB_VIEW_REPO_STRUCTURE'
    );

    expect(desc1).toBeDefined();
    expect(desc1?.enumerable).toBe(true);
    expect(desc1?.configurable).toBe(true);
    expect(typeof desc1?.value).toBe('string');

    expect(desc2).toBeDefined();
    expect(desc3).toBeDefined();
    expect(desc4).toBeDefined();
    expect(desc5).toBeDefined();
  });

  it('should return undefined for non-existent tool names', async () => {
    vi.resetModules();

    const { TOOL_NAMES } =
      await import('../../src/tools/toolMetadata/proxies.js');

    const desc = Object.getOwnPropertyDescriptor(
      TOOL_NAMES,
      'NON_EXISTENT_TOOL'
    );

    expect(desc).toBeUndefined();
  });

  it('should support Object.keys on TOOL_NAMES early', async () => {
    vi.resetModules();

    const { TOOL_NAMES } =
      await import('../../src/tools/toolMetadata/proxies.js');

    const keys = Object.keys(TOOL_NAMES);

    expect(Array.isArray(keys)).toBe(true);
    expect(keys.length).toBeGreaterThan(0);
    expect(keys).toContain('GITHUB_FETCH_CONTENT');
    expect(keys).toContain('GITHUB_SEARCH_CODE');
  });

  it('should support Object.entries on TOOL_NAMES early', async () => {
    vi.resetModules();

    const { TOOL_NAMES } =
      await import('../../src/tools/toolMetadata/proxies.js');

    const entries = Object.entries(TOOL_NAMES);

    expect(Array.isArray(entries)).toBe(true);
    expect(entries.length).toBeGreaterThan(0);
  });
});
