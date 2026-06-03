import { describe, it, expect, beforeEach, vi } from 'vitest';

vi.mock('@octocodeai/octocode-core', () => {
  const config = {
    instructions: 'Test instructions',
    prompts: {},
    toolNames: {
      GITHUB_SEARCH_CODE: 'githubSearchCode',
    },
    baseSchema: {
      mainResearchGoal: 'Main goal',
      researchGoal: 'Research goal',
      reasoning: 'Reasoning',
      bulkQuery: (toolName: string) => 'Query for ' + toolName,
    },
    tools: {
      githubSearchCode: {
        name: 'githubSearchCode',
        description: 'Search code',
        schema: { keyword: 'Keywords to search' },
        hints: {
          hasResults: ['Found results'],
          empty: ['No results'],
        },
      },
    },
    baseHints: {
      hasResults: ['Base result hint'],
      empty: ['Base empty hint'],
    },
    genericErrorHints: ['Error hint'],
  };
  return { octocodeConfig: config, completeMetadata: config };
});

describe('toolMetadata/state', () => {
  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
  });

  describe('getMetadata - cache and reuse', () => {
    it('should return same cached object on second call', async () => {
      const { getMetadata, _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      _resetMetadataState();

      const result1 = await getMetadata();
      const result2 = await getMetadata();

      expect(result1).toBe(result2);
    });

    it('should return identical results on concurrent calls', async () => {
      const { getMetadata, _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      _resetMetadataState();

      const [result1, result2, result3] = await Promise.all([
        getMetadata(),
        getMetadata(),
        getMetadata(),
      ]);

      expect(result1).toBe(result2);
      expect(result2).toBe(result3);
    });
  });

  describe('initializeToolMetadata', () => {
    it('should initialize metadata from octocode-core', async () => {
      const { initializeToolMetadata, getMetadataOrNull, _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      _resetMetadataState();

      await initializeToolMetadata();

      expect(getMetadataOrNull()).not.toBeNull();
      expect(getMetadataOrNull()?.instructions).toBe('Test instructions');
    });

    it('should only initialize once (idempotent)', async () => {
      const {
        initializeToolMetadata,
        getMetadataOrThrow,
        _resetMetadataState,
      } = await import('../../../src/tools/toolMetadata/state.js');
      _resetMetadataState();

      await initializeToolMetadata();
      const first = getMetadataOrThrow();
      await initializeToolMetadata();
      await initializeToolMetadata();
      const second = getMetadataOrThrow();

      expect(first).toBe(second);
    });

    it('should handle concurrent initialization calls', async () => {
      const {
        initializeToolMetadata,
        getMetadataOrThrow,
        _resetMetadataState,
      } = await import('../../../src/tools/toolMetadata/state.js');
      _resetMetadataState();

      await Promise.all([
        initializeToolMetadata(),
        initializeToolMetadata(),
        initializeToolMetadata(),
      ]);

      expect(getMetadataOrThrow()).toBeDefined();
    });
  });

  describe('loadToolContent', () => {
    it('should initialize and return metadata', async () => {
      const { loadToolContent, _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      _resetMetadataState();

      const result = await loadToolContent();

      expect(result).toBeDefined();
      expect(result.instructions).toBe('Test instructions');
      expect(result.toolNames).toBeDefined();
    });

    it('should return cached metadata on subsequent calls', async () => {
      const { loadToolContent, _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      _resetMetadataState();

      const result1 = await loadToolContent();
      const result2 = await loadToolContent();

      expect(result1).toBe(result2);
    });

    it('should return bulkQuery as a function', async () => {
      const { loadToolContent, _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      _resetMetadataState();

      const result = await loadToolContent();

      expect(typeof result.baseSchema.bulkQuery).toBe('function');
      expect(result.baseSchema.bulkQuery('testTool')).toBe(
        'Query for testTool'
      );
    });
  });

  describe('BASE_SCHEMA proxy', () => {
    it('adds the local verbose boolean description when upstream metadata lacks it', async () => {
      const { BASE_SCHEMA } =
        await import('../../../src/tools/toolMetadata/baseSchema.js');

      const base = BASE_SCHEMA as Record<string, unknown>;

      expect(base.verbose).toContain('Boolean detail switch');
      expect(Object.keys(BASE_SCHEMA)).toContain('verbose');
    });
  });

  describe('getMetadataOrThrow', () => {
    it('should throw when metadata not initialized', async () => {
      const { getMetadataOrThrow, _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      _resetMetadataState();

      expect(() => getMetadataOrThrow()).toThrow(
        'Tool metadata not initialized'
      );
    });

    it('should return metadata when initialized', async () => {
      const {
        getMetadataOrThrow,
        initializeToolMetadata,
        _resetMetadataState,
      } = await import('../../../src/tools/toolMetadata/state.js');
      _resetMetadataState();
      await initializeToolMetadata();

      const result = getMetadataOrThrow();

      expect(result).toBeDefined();
      expect(result.instructions).toBe('Test instructions');
    });
  });

  describe('getMetadataOrNull', () => {
    it('should return null when metadata not initialized', async () => {
      const { getMetadataOrNull, _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      _resetMetadataState();

      expect(getMetadataOrNull()).toBeNull();
    });

    it('should return metadata when initialized', async () => {
      const { getMetadataOrNull, initializeToolMetadata, _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      _resetMetadataState();
      await initializeToolMetadata();

      const result = getMetadataOrNull();

      expect(result).not.toBeNull();
      expect(result?.instructions).toBe('Test instructions');
    });
  });

  describe('_resetMetadataState', () => {
    it('should reset all state', async () => {
      const { initializeToolMetadata, getMetadataOrNull, _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');

      await initializeToolMetadata();
      expect(getMetadataOrNull()).not.toBeNull();

      _resetMetadataState();
      expect(getMetadataOrNull()).toBeNull();
    });

    it('should allow re-initialization after reset', async () => {
      const { initializeToolMetadata, getMetadataOrNull, _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');

      await initializeToolMetadata();
      _resetMetadataState();
      await initializeToolMetadata();

      expect(getMetadataOrNull()).not.toBeNull();
    });
  });

  describe('metadata reference stability', () => {
    it('should return the same object on repeated loadToolContent calls', async () => {
      const { loadToolContent, _resetMetadataState } =
        await import('../../../src/tools/toolMetadata/state.js');
      _resetMetadataState();

      const a = await loadToolContent();
      const b = await loadToolContent();

      expect(a).toBe(b);
      expect(a.instructions).toBe('Test instructions');
    });
  });
});
