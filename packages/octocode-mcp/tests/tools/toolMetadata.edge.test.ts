import { describe, it, expect, vi, beforeEach } from 'vitest';

describe('toolMetadata - Final Edge Cases', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.resetModules();
  });

  describe('loadToolContent', () => {
    it('returns core metadata directly', async () => {
      const { loadToolContent } =
        await import('../../../octocode-tools-core/src/tools/toolMetadata/state.js');

      const content = await loadToolContent();

      expect(content).toBeDefined();
      expect(content.toolNames).toBeDefined();
      expect(typeof content.instructions).toBe('string');
    });

    it('is stable across repeated calls', async () => {
      const { loadToolContent } =
        await import('../../../octocode-tools-core/src/tools/toolMetadata/state.js');

      const first = await loadToolContent();
      const second = await loadToolContent();
      expect(second).toBe(first);
    });
  });
});
