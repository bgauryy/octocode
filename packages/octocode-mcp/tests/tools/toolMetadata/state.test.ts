import { describe, it, expect, beforeEach, vi } from 'vitest';

vi.mock('@octocodeai/octocode-core', () => {
  const config = {
    instructions: 'Test instructions',
    prompts: {},
    toolNames: {
      GITHUB_SEARCH_CODE: 'githubSearchCode',
    },
    baseSchema: {
      id: 'Query id',
      mainResearchGoal: 'Main goal',
      researchGoal: 'Research goal',
      reasoning: 'Reasoning',
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

  describe('loadToolContent', () => {
    it('should return core metadata', async () => {
      const { loadToolContent } =
        await import('../../../src/tools/toolMetadata/state.js');

      const result = await loadToolContent();

      expect(result).toBeDefined();
      expect(result.instructions).toBe('Test instructions');
      expect(result.toolNames).toBeDefined();
    });

    it('should return the same object on repeated calls', async () => {
      const { loadToolContent } =
        await import('../../../src/tools/toolMetadata/state.js');

      const result1 = await loadToolContent();
      const result2 = await loadToolContent();

      expect(result1).toBe(result2);
    });

    it('should return base schema string fields', async () => {
      const { loadToolContent } =
        await import('../../../src/tools/toolMetadata/state.js');

      const result = await loadToolContent();

      expect(result.baseSchema.id).toBe('Query id');
      expect(result.baseSchema.mainResearchGoal).toBe('Main goal');
      expect(result.baseSchema.researchGoal).toBe('Research goal');
      expect(result.baseSchema.reasoning).toBe('Reasoning');
    });
  });

  describe('BASE_SCHEMA proxy', () => {
    it('proxies upstream base schema fields', async () => {
      const { BASE_SCHEMA } =
        await import('../../../src/tools/toolMetadata/baseSchema.js');

      expect(typeof BASE_SCHEMA).toBe('object');
      expect(BASE_SCHEMA).not.toBeNull();
      expect(BASE_SCHEMA.mainResearchGoal).toBe('Main goal');
    });
  });

  describe('DESCRIPTIONS proxy', () => {
    it('reads tool descriptions from core metadata', async () => {
      const { DESCRIPTIONS } =
        await import('../../../src/tools/toolMetadata/descriptions.js');

      expect(DESCRIPTIONS['githubSearchCode']).toBe('Search code');
      expect(DESCRIPTIONS['unknownTool']).toBe('');
    });
  });
});
