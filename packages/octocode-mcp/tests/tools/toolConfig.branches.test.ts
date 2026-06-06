import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('../../src/tools/toolMetadata/proxies.js', async importOriginal => {
  const mod =
    await importOriginal<
      typeof import('../../src/tools/toolMetadata/proxies.js')
    >();
  return {
    ...mod,
    DESCRIPTIONS: new Proxy(mod.DESCRIPTIONS as Record<string, string>, {
      get(target, prop: string) {
        if (prop === '__nonexistent_tool_for_coverage__') return undefined;
        return Reflect.get(target, prop) ?? '';
      },
    }),
  };
});

describe('toolConfig branch coverage - getDescription fallback (line 26)', () => {
  beforeEach(async () => {
    vi.resetModules();
  });

  describe('when DESCRIPTIONS returns undefined (fallback branch)', () => {
    it('should return empty string when tool is not in DESCRIPTIONS', async () => {
      const { DESCRIPTIONS } =
        await import('../../src/tools/toolMetadata/proxies.js');

      const unknownDescription = DESCRIPTIONS['completely_unknown_tool_xyz'];
      expect(unknownDescription).toBe('');
    });

    it('should return empty string for undefined tool name', async () => {
      const { DESCRIPTIONS } =
        await import('../../src/tools/toolMetadata/proxies.js');

      const result = DESCRIPTIONS[''];
      expect(result).toBe('');
    });

    it('getDescription returns empty string for unknown tool (hits ?? fallback)', async () => {
      const { getDescription } = await import('../../src/tools/toolConfig.js');
      const result = getDescription('__nonexistent_tool_for_coverage__');
      expect(result).toBe('');
    });
  });

  describe('tool configuration initialization', () => {
    it('should create valid tool configs with all required properties', async () => {
      const {
        GITHUB_SEARCH_CODE,
        GITHUB_FETCH_CONTENT,
        GITHUB_VIEW_REPO_STRUCTURE,
        GITHUB_SEARCH_REPOSITORIES,
        GITHUB_SEARCH_PULL_REQUESTS,
        PACKAGE_SEARCH,
        ALL_TOOLS,
      } = await import('../../src/tools/toolConfig.js');

      const configs = [
        GITHUB_SEARCH_CODE,
        GITHUB_FETCH_CONTENT,
        GITHUB_VIEW_REPO_STRUCTURE,
        GITHUB_SEARCH_REPOSITORIES,
        GITHUB_SEARCH_PULL_REQUESTS,
        PACKAGE_SEARCH,
      ];

      for (const config of configs) {
        expect(config).toHaveProperty('name');
        expect(config).toHaveProperty('description');
        expect(config).toHaveProperty('isDefault');
        expect(config).toHaveProperty('type');
        expect(config).toHaveProperty('fn');
        expect(typeof config.name).toBe('string');
        expect(typeof config.description).toBe('string');
        expect(typeof config.isDefault).toBe('boolean');
        expect(typeof config.type).toBe('string');
        expect(typeof config.fn).toBe('function');
      }

      expect(ALL_TOOLS).toHaveLength(14);
    });

    it('should have correct tool types assigned', async () => {
      const {
        GITHUB_SEARCH_CODE,
        GITHUB_FETCH_CONTENT,
        GITHUB_VIEW_REPO_STRUCTURE,
        GITHUB_SEARCH_REPOSITORIES,
        GITHUB_SEARCH_PULL_REQUESTS,
        PACKAGE_SEARCH,
      } = await import('../../src/tools/toolConfig.js');

      expect(GITHUB_SEARCH_CODE.type).toBe('search');
      expect(GITHUB_SEARCH_REPOSITORIES.type).toBe('search');
      expect(PACKAGE_SEARCH.type).toBe('search');

      expect(GITHUB_FETCH_CONTENT.type).toBe('content');
      expect(GITHUB_VIEW_REPO_STRUCTURE.type).toBe('content');

      expect(GITHUB_SEARCH_PULL_REQUESTS.type).toBe('history');
    });

    it('should mark all tools as default', async () => {
      const { ALL_TOOLS } = await import('../../src/tools/toolConfig.js');

      for (const tool of ALL_TOOLS) {
        expect(tool.isDefault).toBe(true);
      }
    });
  });
});

describe('toolConfig - fn property', () => {
  it('should have callable registration functions', async () => {
    const {
      GITHUB_SEARCH_CODE,
      GITHUB_FETCH_CONTENT,
      GITHUB_VIEW_REPO_STRUCTURE,
      GITHUB_SEARCH_REPOSITORIES,
      GITHUB_SEARCH_PULL_REQUESTS,
      PACKAGE_SEARCH,
    } = await import('../../src/tools/toolConfig.js');

    expect(typeof GITHUB_SEARCH_CODE.fn).toBe('function');
    expect(typeof GITHUB_FETCH_CONTENT.fn).toBe('function');
    expect(typeof GITHUB_VIEW_REPO_STRUCTURE.fn).toBe('function');
    expect(typeof GITHUB_SEARCH_REPOSITORIES.fn).toBe('function');
    expect(typeof GITHUB_SEARCH_PULL_REQUESTS.fn).toBe('function');
    expect(typeof PACKAGE_SEARCH.fn).toBe('function');
  });
});
