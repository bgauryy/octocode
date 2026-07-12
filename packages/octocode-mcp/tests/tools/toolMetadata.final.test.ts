import { describe, it, expect, vi, beforeEach } from 'vitest';

const emptyCompleteMetadata = {
  systemPrompt: '',
  prompts: {},
  toolNames: {
    GITHUB_FETCH_CONTENT: 'ghGetFileContent',
    GITHUB_SEARCH_CODE: 'ghSearchCode',
    GITHUB_SEARCH_REPOSITORIES: 'ghSearchRepos',
    GITHUB_SEARCH_PULL_REQUESTS: 'ghHistoryResearch',
    GITHUB_VIEW_REPO_STRUCTURE: 'ghViewRepoStructure',
    PACKAGE_SEARCH: 'npmSearch',
    LOCAL_RIPGREP: 'localSearchCode',
    LOCAL_FETCH_CONTENT: 'localGetFileContent',
    LOCAL_FIND_FILES: 'localFindFiles',
    LOCAL_VIEW_STRUCTURE: 'localViewStructure',
    LSP_GET_SEMANTIC_CONTENT: 'lspGetSemantics',
  },
  baseSchema: {
    mainResearchGoal: '',
    researchGoal: '',
    reasoning: '',
    bulkQuery: () => '',
  },
  tools: {},
  baseHints: { hasResults: [], empty: [] },
  genericErrorHints: [],
  bulkOperations: {},
};

vi.mock('@octocodeai/octocode-core', () => ({
  octocodeConfig: {},
  completeMetadata: emptyCompleteMetadata,
}));

describe('toolMetadata - TOOL_NAMES static fallback (lines 236-243)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.resetModules();
  });

  it('should use STATIC_TOOL_NAMES when metadata not loaded', async () => {
    vi.resetModules();

    const { TOOL_NAMES } =
      await import('../../../octocode-tools-core/src/tools/toolMetadata/proxies.js');

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
      await import('../../../octocode-tools-core/src/tools/toolMetadata/proxies.js');

    const desc = Object.getOwnPropertyDescriptor(
      TOOL_NAMES,
      'NON_EXISTENT_TOOL'
    );

    expect(desc).toBeUndefined();
  });

  it('should support Object.keys on TOOL_NAMES early', async () => {
    vi.resetModules();

    const { TOOL_NAMES } =
      await import('../../../octocode-tools-core/src/tools/toolMetadata/proxies.js');

    const keys = Object.keys(TOOL_NAMES);

    expect(Array.isArray(keys)).toBe(true);
    expect(keys.length).toBeGreaterThan(0);
    expect(keys).toContain('GITHUB_FETCH_CONTENT');
    expect(keys).toContain('GITHUB_SEARCH_CODE');
  });

  it('should support Object.entries on TOOL_NAMES early', async () => {
    vi.resetModules();

    const { TOOL_NAMES } =
      await import('../../../octocode-tools-core/src/tools/toolMetadata/proxies.js');

    const entries = Object.entries(TOOL_NAMES);

    expect(Array.isArray(entries)).toBe(true);
    expect(entries.length).toBeGreaterThan(0);
  });
});
