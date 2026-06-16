import { describe, it, expect, beforeEach, vi } from 'vitest';

describe('toolMetadata/proxies', () => {
  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
  });

  describe('TOOL_NAMES proxy', () => {
    it('should return tool names from metadata', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/state.js');
      const { TOOL_NAMES } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/proxies.js');
      _resetMetadataState();
      await initializeToolMetadata();

      expect(TOOL_NAMES.GITHUB_SEARCH_CODE).toBe('ghSearchCode');
    });

    it('should fallback to static names when not initialized', async () => {
      const { _resetMetadataState } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/state.js');
      const { TOOL_NAMES } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/proxies.js');
      _resetMetadataState();

      expect(typeof TOOL_NAMES.GITHUB_SEARCH_CODE).toBe('string');
    });

    it('should support Object.keys enumeration', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/state.js');
      const { TOOL_NAMES } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/proxies.js');
      _resetMetadataState();
      await initializeToolMetadata();

      const keys = Object.keys(TOOL_NAMES);
      expect(keys).toContain('GITHUB_SEARCH_CODE');
    });

    it('should support getOwnPropertyDescriptor', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/state.js');
      const { TOOL_NAMES } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/proxies.js');
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
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/state.js');
      const { TOOL_NAMES } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/proxies.js');
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
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/state.js');
      const { BASE_SCHEMA } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/proxies.js');
      _resetMetadataState();
      await initializeToolMetadata();

      expect(typeof BASE_SCHEMA.mainResearchGoal).toBe('string');
      expect(BASE_SCHEMA.mainResearchGoal.length).toBeGreaterThan(0);
      expect(typeof BASE_SCHEMA.researchGoal).toBe('string');
      expect(BASE_SCHEMA.researchGoal.length).toBeGreaterThan(0);
    });

    it('should read base schema from completeMetadata when state is reset', async () => {
      const { _resetMetadataState } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/state.js');
      const { BASE_SCHEMA } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/proxies.js');
      _resetMetadataState();

      expect(typeof BASE_SCHEMA.mainResearchGoal).toBe('string');
      expect(BASE_SCHEMA.mainResearchGoal.length).toBeGreaterThan(0);
    });
  });

  describe('GENERIC_ERROR_HINTS proxy', () => {
    it('should return error hints after init', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/state.js');
      const { GENERIC_ERROR_HINTS } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/proxies.js');
      _resetMetadataState();
      await initializeToolMetadata();

      expect(Array.isArray(GENERIC_ERROR_HINTS)).toBe(true);
    });
  });

  describe('DESCRIPTIONS proxy', () => {
    it('should return tool description', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/state.js');
      const { DESCRIPTIONS } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/proxies.js');
      _resetMetadataState();
      await initializeToolMetadata();

      expect(typeof DESCRIPTIONS['ghSearchCode']).toBe('string');
      expect(DESCRIPTIONS['ghSearchCode'].length).toBeGreaterThan(0);
    });

    it('should return empty string for unknown tool', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/state.js');
      const { DESCRIPTIONS } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/proxies.js');
      _resetMetadataState();
      await initializeToolMetadata();

      expect(DESCRIPTIONS['unknownTool']).toBe('');
    });
  });

  describe('TOOL_HINTS proxy', () => {
    it('should return tool hints', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/state.js');
      const { TOOL_HINTS } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/proxies.js');
      _resetMetadataState();
      await initializeToolMetadata();

      const hints = TOOL_HINTS['ghSearchCode'];
      expect(hints).toBeDefined();
      expect(typeof hints).toBe('object');
    });

    it('should return base hints', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/state.js');
      const { TOOL_HINTS } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/proxies.js');
      _resetMetadataState();
      await initializeToolMetadata();

      const base = TOOL_HINTS.base;
      expect(base).toBeDefined();
      expect(typeof base).toBe('object');
    });

    it('should return empty hints for unknown tool', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/state.js');
      const { TOOL_HINTS } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/proxies.js');
      _resetMetadataState();
      await initializeToolMetadata();

      const hints = TOOL_HINTS['unknown'];
      expect(hints?.hasResults).toEqual([]);
      expect(hints?.empty).toEqual([]);
    });

    it('should support Object.keys', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/state.js');
      const { TOOL_HINTS } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/proxies.js');
      _resetMetadataState();
      await initializeToolMetadata();

      const keys = Object.keys(TOOL_HINTS);
      expect(keys).toContain('base');
      expect(keys).toContain('ghSearchCode');
    });

    it('should support Object.keys when metadata is null (ownKeys)', async () => {
      const { _resetMetadataState } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/state.js');
      const { TOOL_HINTS } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/proxies.js');
      _resetMetadataState();

      const keys = Object.keys(TOOL_HINTS);
      expect(keys).toContain('base');
    });

    it('getOwnPropertyDescriptor returns undefined for unknown key on TOOL_HINTS', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/state.js');
      const { TOOL_HINTS } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/proxies.js');
      _resetMetadataState();
      await initializeToolMetadata();

      const descriptor = Object.getOwnPropertyDescriptor(
        TOOL_HINTS,
        'totally_unknown_tool_xyz'
      );
      expect(descriptor).toBeUndefined();
    });
  });

  describe('isToolInMetadata', () => {
    it('should return true for existing tool', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/state.js');
      const { isToolInMetadata } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/proxies.js');
      _resetMetadataState();
      await initializeToolMetadata();

      expect(isToolInMetadata('ghSearchCode')).toBe(true);
    });

    it('should return false for non-existent tool', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/state.js');
      const { isToolInMetadata } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/proxies.js');
      _resetMetadataState();
      await initializeToolMetadata();

      expect(isToolInMetadata('unknownTool')).toBe(false);
    });

    it('should use completeMetadata.tools when state is reset', async () => {
      const { _resetMetadataState } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/state.js');
      const { isToolInMetadata } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/proxies.js');
      _resetMetadataState();

      expect(isToolInMetadata('ghSearchCode')).toBe(true);
    });
  });

  describe('getToolHintsSync', () => {
    it('should return tool hints only (base hints in server.instructions)', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/state.js');
      const { getToolHintsSync } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/proxies.js');
      _resetMetadataState();
      await initializeToolMetadata();

      const hints = getToolHintsSync('ghSearchCode', 'empty');
      expect(Array.isArray(hints)).toBe(true);
    });

    it('should return tool hints only for local tools', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/state.js');
      const { getToolHintsSync } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/proxies.js');
      _resetMetadataState();
      await initializeToolMetadata();

      const hints = getToolHintsSync('localSearchCode', 'empty');
      expect(Array.isArray(hints)).toBe(true);
    });

    it('should return empty array for unknown tool', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/state.js');
      const { getToolHintsSync } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/proxies.js');
      _resetMetadataState();
      await initializeToolMetadata();

      const hints = getToolHintsSync('unknown', 'hasResults');
      expect(hints).toEqual([]);
    });
  });

  describe('getGenericErrorHintsSync', () => {
    it('should return error hints', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/state.js');
      const { getGenericErrorHintsSync } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/proxies.js');
      _resetMetadataState();
      await initializeToolMetadata();

      const hints = getGenericErrorHintsSync();
      expect(Array.isArray(hints)).toBe(true);
    });

    it('should return generic hints from completeMetadata when state is reset', async () => {
      const { _resetMetadataState } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/state.js');
      const { getGenericErrorHintsSync } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/proxies.js');
      _resetMetadataState();

      const hints = getGenericErrorHintsSync();
      expect(Array.isArray(hints)).toBe(true);
    });
  });

  describe('getDynamicHints', () => {
    it('should return dynamic hints', async () => {
      const { initializeToolMetadata, _resetMetadataState } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/state.js');
      const { getDynamicHints } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/proxies.js');
      _resetMetadataState();
      await initializeToolMetadata();

      const hints = getDynamicHints('ghSearchCode', 'topicsHasResults');
      expect(Array.isArray(hints)).toBe(true);
    });

    it('should return dynamic hints from completeMetadata when state is reset', async () => {
      const { _resetMetadataState } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/state.js');
      const { getDynamicHints } =
        await import('../../../../octocode-tools-core/src/tools/toolMetadata/proxies.js');
      _resetMetadataState();

      const hints = getDynamicHints('ghSearchCode', 'topicsHasResults');
      expect(Array.isArray(hints)).toBe(true);
    });
  });
});
