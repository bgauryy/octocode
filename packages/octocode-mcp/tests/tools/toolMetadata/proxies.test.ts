import { describe, it, expect, beforeEach, vi } from 'vitest';

const mockMetadata = {
  instructions: 'Test instructions',
  prompts: {},
  toolNames: {
    GITHUB_SEARCH_CODE: 'githubSearchCode',
    LOCAL_RIPGREP: 'localSearchCode',
  },
  baseSchema: {
    mainResearchGoal: 'Main goal',
    researchGoal: 'Research goal',
    reasoning: 'Reasoning',
    bulkQueryTemplate: 'Query for {toolName}',
    bulkQuery: (toolName: string) => 'Query for ' + toolName,
  },
  tools: {
    githubSearchCode: {
      name: 'githubSearchCode',
      description: 'Search code on GitHub',
      schema: { keyword: 'Keywords' },
      hints: {
        hasResults: ['GitHub hint'],
        empty: ['No GitHub results'],
        dynamic: {
          topicsHasResults: ['Dynamic topic hint'],
        },
      },
    },
    localSearchCode: {
      name: 'localSearchCode',
      description: 'Search local code',
      schema: { pattern: 'Pattern' },
      hints: {
        hasResults: ['Local code hint'],
        empty: ['No local results'],
      },
    },
  },
  baseHints: {
    hasResults: ['Base result hint'],
    empty: ['Base empty hint'],
  },
  genericErrorHints: ['Generic error 1', 'Generic error 2'],
};

vi.mock('@octocodeai/octocode-core', () => ({
  octocodeConfig: mockMetadata,
  completeMetadata: mockMetadata,
}));

describe('toolMetadata/proxies', () => {
  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
  });

  describe('TOOL_NAMES proxy', () => {
    it('should return tool names from metadata', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      const { TOOL_NAMES } =
        await import('../../../src/tools/toolMetadata/proxies.js');
      _resetMetadataState();
      await initializeToolMetadata();

      expect(TOOL_NAMES.GITHUB_SEARCH_CODE).toBe('githubSearchCode');
    });

    it('should fallback to static names when not initialized', async () => {
      const { _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      const { TOOL_NAMES } =
        await import('../../../src/tools/toolMetadata/proxies.js');
      _resetMetadataState();

      expect(typeof TOOL_NAMES.GITHUB_SEARCH_CODE).toBe('string');
    });

    it('should support Object.keys enumeration', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      const { TOOL_NAMES } =
        await import('../../../src/tools/toolMetadata/proxies.js');
      _resetMetadataState();
      await initializeToolMetadata();

      const keys = Object.keys(TOOL_NAMES);
      expect(keys).toContain('GITHUB_SEARCH_CODE');
    });

    it('should support getOwnPropertyDescriptor', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      const { TOOL_NAMES } =
        await import('../../../src/tools/toolMetadata/proxies.js');
      _resetMetadataState();
      await initializeToolMetadata();

      const descriptor = Object.getOwnPropertyDescriptor(
        TOOL_NAMES,
        'GITHUB_SEARCH_CODE'
      );
      expect(descriptor?.enumerable).toBe(true);
      expect(descriptor?.configurable).toBe(true);
    });

    it('should return undefined for unknown tools', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      const { TOOL_NAMES } =
        await import('../../../src/tools/toolMetadata/proxies.js');
      _resetMetadataState();
      await initializeToolMetadata();

      const descriptor = Object.getOwnPropertyDescriptor(
        TOOL_NAMES,
        'UNKNOWN_TOOL'
      );
      expect(descriptor).toBeUndefined();
    });
  });

  describe('BASE_SCHEMA proxy', () => {
    it('should return schema fields after init', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      const { BASE_SCHEMA } =
        await import('../../../src/tools/toolMetadata/proxies.js');
      _resetMetadataState();
      await initializeToolMetadata();

      expect(BASE_SCHEMA.mainResearchGoal).toBe('Main goal');
      expect(BASE_SCHEMA.researchGoal).toBe('Research goal');
    });

    it('should provide bulkQuery function', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      const { BASE_SCHEMA } =
        await import('../../../src/tools/toolMetadata/proxies.js');
      _resetMetadataState();
      await initializeToolMetadata();

      const result = BASE_SCHEMA.bulkQuery('myTool');
      expect(result).toBe('Query for myTool');
    });

    it('should provide fallback bulkQuery when not initialized', async () => {
      const { _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      const { BASE_SCHEMA } =
        await import('../../../src/tools/toolMetadata/proxies.js');
      _resetMetadataState();

      const result = BASE_SCHEMA.bulkQuery('testTool');
      expect(result).toContain('testTool');
    });

    it('should read base schema from completeMetadata when state is reset', async () => {
      const { _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      const { BASE_SCHEMA } =
        await import('../../../src/tools/toolMetadata/proxies.js');
      _resetMetadataState();

      expect(BASE_SCHEMA.mainResearchGoal).toBe('Main goal');
    });
  });

  describe('GENERIC_ERROR_HINTS proxy', () => {
    it('should return error hints after init', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      const { GENERIC_ERROR_HINTS } =
        await import('../../../src/tools/toolMetadata/proxies.js');
      _resetMetadataState();
      await initializeToolMetadata();

      expect(GENERIC_ERROR_HINTS.length).toBe(2);
    });
  });

  describe('DESCRIPTIONS proxy', () => {
    it('should return tool description', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      const { DESCRIPTIONS } =
        await import('../../../src/tools/toolMetadata/proxies.js');
      _resetMetadataState();
      await initializeToolMetadata();

      expect(DESCRIPTIONS['githubSearchCode']).toBe('Search code on GitHub');
    });

    it('should return empty string for unknown tool', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      const { DESCRIPTIONS } =
        await import('../../../src/tools/toolMetadata/proxies.js');
      _resetMetadataState();
      await initializeToolMetadata();

      expect(DESCRIPTIONS['unknownTool']).toBe('');
    });
  });

  describe('TOOL_HINTS proxy', () => {
    it('should return tool hints', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      const { TOOL_HINTS } =
        await import('../../../src/tools/toolMetadata/proxies.js');
      _resetMetadataState();
      await initializeToolMetadata();

      const hints = TOOL_HINTS['githubSearchCode'];
      expect(hints?.hasResults).toContain('GitHub hint');
    });

    it('should return base hints', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      const { TOOL_HINTS } =
        await import('../../../src/tools/toolMetadata/proxies.js');
      _resetMetadataState();
      await initializeToolMetadata();

      const base = TOOL_HINTS.base;
      expect(base.hasResults).toContain('Base result hint');
    });

    it('should return empty hints for unknown tool', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      const { TOOL_HINTS } =
        await import('../../../src/tools/toolMetadata/proxies.js');
      _resetMetadataState();
      await initializeToolMetadata();

      const hints = TOOL_HINTS['unknown'];
      expect(hints?.hasResults).toEqual([]);
      expect(hints?.empty).toEqual([]);
    });

    it('should support Object.keys', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      const { TOOL_HINTS } =
        await import('../../../src/tools/toolMetadata/proxies.js');
      _resetMetadataState();
      await initializeToolMetadata();

      const keys = Object.keys(TOOL_HINTS);
      expect(keys).toContain('base');
      expect(keys).toContain('githubSearchCode');
    });

    it('should support Object.keys when metadata is null (ownKeys)', async () => {
      const { _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      const { TOOL_HINTS } =
        await import('../../../src/tools/toolMetadata/proxies.js');
      _resetMetadataState();

      const keys = Object.keys(TOOL_HINTS);
      expect(keys).toContain('base');
    });
  });

  describe('isToolInMetadata', () => {
    it('should return true for existing tool', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      const { isToolInMetadata } =
        await import('../../../src/tools/toolMetadata/proxies.js');
      _resetMetadataState();
      await initializeToolMetadata();

      expect(isToolInMetadata('githubSearchCode')).toBe(true);
    });

    it('should return false for non-existent tool', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      const { isToolInMetadata } =
        await import('../../../src/tools/toolMetadata/proxies.js');
      _resetMetadataState();
      await initializeToolMetadata();

      expect(isToolInMetadata('unknownTool')).toBe(false);
    });

    it('should use completeMetadata.tools when state is reset', async () => {
      const { _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      const { isToolInMetadata } =
        await import('../../../src/tools/toolMetadata/proxies.js');
      _resetMetadataState();

      expect(isToolInMetadata('githubSearchCode')).toBe(true);
    });
  });

  describe('getToolHintsSync', () => {
    it('should return tool hints only (base hints in server.instructions)', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      const { getToolHintsSync } =
        await import('../../../src/tools/toolMetadata/proxies.js');
      _resetMetadataState();
      await initializeToolMetadata();

      const hints = getToolHintsSync('githubSearchCode', 'hasResults');
      expect(hints).toContain('GitHub hint');
    });

    it('should return tool hints only for local tools', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      const { getToolHintsSync } =
        await import('../../../src/tools/toolMetadata/proxies.js');
      _resetMetadataState();
      await initializeToolMetadata();

      const hints = getToolHintsSync('localSearchCode', 'hasResults');
      expect(hints).toContain('Local code hint');
    });

    it('should return empty array for unknown tool', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      const { getToolHintsSync } =
        await import('../../../src/tools/toolMetadata/proxies.js');
      _resetMetadataState();
      await initializeToolMetadata();

      const hints = getToolHintsSync('unknown', 'hasResults');
      expect(hints).toEqual([]);
    });
  });

  describe('getGenericErrorHintsSync', () => {
    it('should return error hints', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      const { getGenericErrorHintsSync } =
        await import('../../../src/tools/toolMetadata/proxies.js');
      _resetMetadataState();
      await initializeToolMetadata();

      const hints = getGenericErrorHintsSync();
      expect(hints).toContain('Generic error 1');
    });

    it('should return generic hints from completeMetadata when state is reset', async () => {
      const { _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      const { getGenericErrorHintsSync } =
        await import('../../../src/tools/toolMetadata/proxies.js');
      _resetMetadataState();

      const hints = getGenericErrorHintsSync();
      expect(hints).toContain('Generic error 1');
    });
  });

  describe('getDynamicHints', () => {
    it('should return dynamic hints', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      const { getDynamicHints } =
        await import('../../../src/tools/toolMetadata/proxies.js');
      _resetMetadataState();
      await initializeToolMetadata();

      const hints = getDynamicHints('githubSearchCode', 'topicsHasResults');
      expect(hints).toContain('Dynamic topic hint');
    });

    it('should return empty for missing hint type', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      const { getDynamicHints } =
        await import('../../../src/tools/toolMetadata/proxies.js');
      _resetMetadataState();
      await initializeToolMetadata();

      const hints = getDynamicHints('githubSearchCode', 'nonexistent');
      expect(hints).toEqual([]);
    });

    it('should return dynamic hints from completeMetadata when state is reset', async () => {
      const { _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      const { getDynamicHints } =
        await import('../../../src/tools/toolMetadata/proxies.js');
      _resetMetadataState();

      const hints = getDynamicHints('githubSearchCode', 'topicsHasResults');
      expect(hints).toContain('Dynamic topic hint');
    });
  });
});
