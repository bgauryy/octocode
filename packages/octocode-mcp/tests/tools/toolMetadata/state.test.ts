import { describe, it, expect, beforeEach, vi } from 'vitest';

describe('toolMetadata/state', () => {
  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
  });

  describe('loadToolContent', () => {
    it('should return core metadata', async () => {
      const { loadToolContent } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/state.js');

      const result = await loadToolContent();

      expect(result).toBeDefined();
      expect(typeof result.instructions).toBe('string');
      expect(result.toolNames).toBeDefined();
    });

    it('should return the same object on repeated calls', async () => {
      const { loadToolContent } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/state.js');

      const result1 = await loadToolContent();
      const result2 = await loadToolContent();

      expect(result1).toBe(result2);
    });

    it('should return base schema string fields', async () => {
      const { loadToolContent } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/state.js');

      const result = await loadToolContent();

      expect(typeof result.baseSchema.id).toBe('string');
      expect(result.baseSchema.id.length).toBeGreaterThan(0);
      expect(typeof result.baseSchema.mainResearchGoal).toBe('string');
      expect(result.baseSchema.mainResearchGoal.length).toBeGreaterThan(0);
      expect(typeof result.baseSchema.researchGoal).toBe('string');
      expect(typeof result.baseSchema.reasoning).toBe('string');
    });
  });

  describe('BASE_SCHEMA proxy', () => {
    it('proxies upstream base schema fields', async () => {
      const { BASE_SCHEMA } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/baseSchema.js');

      expect(typeof BASE_SCHEMA).toBe('object');
      expect(BASE_SCHEMA).not.toBeNull();
      expect(typeof BASE_SCHEMA.mainResearchGoal).toBe('string');
      expect(BASE_SCHEMA.mainResearchGoal.length).toBeGreaterThan(0);
    });
  });

  describe('DESCRIPTIONS proxy', () => {
    it('reads tool descriptions from core metadata', async () => {
      const { DESCRIPTIONS } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/descriptions.js');

      expect(typeof DESCRIPTIONS['ghSearchCode']).toBe('string');
      expect(DESCRIPTIONS['ghSearchCode'].length).toBeGreaterThan(0);
      expect(DESCRIPTIONS['unknownTool']).toBe('');
    });
  });
});
