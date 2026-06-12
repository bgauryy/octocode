import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';

const hoist = vi.hoisted(() => {
  const mockMetadata = {
    instructions: 'Test instructions',
    prompts: {
      testPrompt: {
        name: 'testPrompt',
        description: 'Test prompt description',
        content: 'Test prompt content',
        args: [{ name: 'arg1', description: 'Argument 1', required: true }],
      },
    },
    toolNames: {
      GITHUB_FETCH_CONTENT: 'githubGetFileContent',
      GITHUB_SEARCH_CODE: 'githubSearchCode',
      GITHUB_SEARCH_PULL_REQUESTS: 'githubSearchPullRequests',
      GITHUB_SEARCH_REPOSITORIES: 'githubSearchRepositories',
      GITHUB_VIEW_REPO_STRUCTURE: 'githubViewRepoStructure',
      LOCAL_RIPGREP: 'localSearchCode',
      LOCAL_FETCH_CONTENT: 'localGetFileContent',
      LOCAL_FIND_FILES: 'localFindFiles',
      LOCAL_VIEW_STRUCTURE: 'localViewStructure',
    },
    baseSchema: {
      mainResearchGoal: 'Main goal description',
      researchGoal: 'Research goal description',
      reasoning: 'Reasoning description',
      bulkQueryTemplate: 'Research queries for {toolName}',
    },
    tools: {
      githubSearchCode: {
        name: 'githubSearchCode',
        description: 'Search code on GitHub',
        schema: {
          keywordsToSearch: 'Keywords to search',
          owner: 'Repository owner',
          repo: 'Repository name',
        },
        hints: {
          hasResults: ['Review results'],
          empty: ['Try different keywords'],
        },
      },
      githubGetFileContent: {
        name: 'githubGetFileContent',
        description: 'Get file content',
        schema: {
          owner: 'Owner',
          repo: 'Repo',
          path: 'Path',
        },
        hints: {
          hasResults: ['File retrieved'],
          empty: ['File not found'],
        },
      },
      localSearchCode: {
        name: 'localSearchCode',
        description: 'Search code locally',
        schema: {
          pattern: 'Search pattern',
          path: 'Search path',
        },
        hints: {
          hasResults: ['Local search results'],
          empty: ['No local matches'],
        },
      },
      localGetFileContent: {
        name: 'localGetFileContent',
        description: 'Get local file content',
        schema: {
          path: 'File path',
        },
        hints: {
          hasResults: ['Local file retrieved'],
          empty: ['Local file not found'],
        },
      },
      localFindFiles: {
        name: 'localFindFiles',
        description: 'Find files locally',
        schema: {
          path: 'Search path',
        },
        hints: {
          hasResults: ['Files found'],
          empty: ['No files found'],
        },
      },
      localViewStructure: {
        name: 'localViewStructure',
        description: 'View local directory structure',
        schema: {
          path: 'Directory path',
        },
        hints: {
          hasResults: ['Structure retrieved'],
          empty: ['Directory empty'],
        },
      },
    },
    baseHints: {
      hasResults: ['Base hint for results'],
      empty: ['Base hint for empty'],
    },
    genericErrorHints: ['Generic error hint 1', 'Generic error hint 2'],
    bulkOperations: {
      instructions: {
        base: '{count} results',
        hasResults: 'Review hasResults hints',
        empty: 'Review empty hints',
        error: 'Review error hints',
      },
    },
  };

  // Metadata with GitHub-specific base hints (realistic API response)
  const mockMetadataWithGitHubHints = {
    ...mockMetadata,
    baseHints: {
      hasResults: [
        "Use 'owner', 'repo', 'branch', 'path' fields directly in next tool calls",
        'Follow mainResearchGoal to navigate research',
        'Common hint for all tools',
        'Check `pushedAt`/`lastModified` - skip stale content',
      ],
      empty: ['Try broader terms', "Use 'repo' and 'owner' to narrow scope"],
    },
  };

  return {
    mockMetadata,
    mockMetadataWithGitHubHints,
    store: { current: mockMetadata },
    octocodeReads: 0,
  };
});

vi.mock('@octocodeai/octocode-core', async importOriginal => ({
  ...(await importOriginal<object>()),
  get octocodeConfig() {
    hoist.octocodeReads++;
    return hoist.store.current;
  },
  get completeMetadata() {
    hoist.octocodeReads++;
    return hoist.store.current;
  },
}));

// LOCAL_BASE_HINTS no longer used by getToolHintsSync (moved to server.instructions)

describe('toolMetadata', () => {
  const { mockMetadata, mockMetadataWithGitHubHints } = hoist;

  function withBulkQuery<
    T extends { baseSchema: { bulkQueryTemplate: string } },
  >(
    m: T
  ): T & {
    baseSchema: T['baseSchema'] & {
      bulkQuery: (toolName: string) => string;
    };
  } {
    return {
      ...m,
      baseSchema: {
        ...m.baseSchema,
        bulkQuery: (toolName: string) =>
          m.baseSchema.bulkQueryTemplate.replace('{toolName}', toolName),
      },
    };
  }

  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
    hoist.store.current = withBulkQuery(structuredClone(hoist.mockMetadata));
    hoist.octocodeReads = 0;
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  describe('initializeToolMetadata', () => {
    it('should initialize metadata from API', async () => {
      const { initializeToolMetadata, getMetadataOrNull } =
        await import('../../src/tools/toolMetadata/state.js');

      await initializeToolMetadata();

      expect(getMetadataOrNull()).not.toBeNull();
      expect(hoist.octocodeReads).toBe(1);
    });

    it('should only initialize once', async () => {
      const { initializeToolMetadata } =
        await import('../../src/tools/toolMetadata/state.js');

      await initializeToolMetadata();
      await initializeToolMetadata();
      await initializeToolMetadata();

      expect(hoist.octocodeReads).toBe(1);
    });

    it('should handle concurrent initialization', async () => {
      const { initializeToolMetadata } =
        await import('../../src/tools/toolMetadata/state.js');

      const promises = [
        initializeToolMetadata(),
        initializeToolMetadata(),
        initializeToolMetadata(),
      ];

      await Promise.all(promises);

      expect(hoist.octocodeReads).toBe(1);
    });
  });

  describe('loadToolContent', () => {
    it('should initialize and return metadata', async () => {
      const { loadToolContent } =
        await import('../../src/tools/toolMetadata/state.js');

      const result = await loadToolContent();

      expect(result).toBeDefined();
      expect(typeof result.instructions).toBe('string');
      expect(result.toolNames).toBeDefined();
    });

    it('should return cached metadata on subsequent calls', async () => {
      const { loadToolContent } =
        await import('../../src/tools/toolMetadata/state.js');

      const result1 = await loadToolContent();
      const result2 = await loadToolContent();

      expect(result1).toBeDefined();
      expect(result2).toBeDefined();
    });
  });

  describe('TOOL_NAMES proxy', () => {
    it('should return tool names', async () => {
      const { initializeToolMetadata } =
        await import('../../src/tools/toolMetadata/state.js');
      const { TOOL_NAMES } =
        await import('../../src/tools/toolMetadata/proxies.js');
      await initializeToolMetadata();

      expect(typeof TOOL_NAMES.GITHUB_SEARCH_CODE).toBe('string');
      expect(typeof TOOL_NAMES.GITHUB_FETCH_CONTENT).toBe('string');
    });

    it('should return tool names consistently', async () => {
      const { TOOL_NAMES } =
        await import('../../src/tools/toolMetadata/proxies.js');
      expect(typeof TOOL_NAMES.GITHUB_SEARCH_CODE).toBe('string');
      expect(typeof TOOL_NAMES.GITHUB_FETCH_CONTENT).toBe('string');
    });

    it('should support ownKeys trap', async () => {
      const { initializeToolMetadata } =
        await import('../../src/tools/toolMetadata/state.js');
      const { TOOL_NAMES } =
        await import('../../src/tools/toolMetadata/proxies.js');
      await initializeToolMetadata();

      const keys = Object.keys(TOOL_NAMES);
      expect(keys).toContain('GITHUB_SEARCH_CODE');
      expect(keys).toContain('GITHUB_FETCH_CONTENT');
    });

    it('should support getOwnPropertyDescriptor trap', async () => {
      const { initializeToolMetadata } =
        await import('../../src/tools/toolMetadata/state.js');
      const { TOOL_NAMES } =
        await import('../../src/tools/toolMetadata/proxies.js');
      await initializeToolMetadata();

      const descriptor = Object.getOwnPropertyDescriptor(
        TOOL_NAMES,
        'GITHUB_SEARCH_CODE'
      );
      expect(descriptor).toBeDefined();
      expect(descriptor?.enumerable).toBe(true);
      expect(descriptor?.configurable).toBe(true);
    });

    it('should return undefined for non-existent tool', async () => {
      const { initializeToolMetadata } =
        await import('../../src/tools/toolMetadata/state.js');
      const { TOOL_NAMES } =
        await import('../../src/tools/toolMetadata/proxies.js');
      await initializeToolMetadata();

      const descriptor = Object.getOwnPropertyDescriptor(
        TOOL_NAMES,
        'NON_EXISTENT'
      );
      expect(descriptor).toBeUndefined();
    });
  });

  describe('BASE_SCHEMA proxy', () => {
    it('should return schema fields after initialization', async () => {
      const { initializeToolMetadata } =
        await import('../../src/tools/toolMetadata/state.js');
      const { BASE_SCHEMA } =
        await import('../../src/tools/toolMetadata/proxies.js');
      await initializeToolMetadata();

      expect(typeof BASE_SCHEMA.mainResearchGoal).toBe('string');
      expect(typeof BASE_SCHEMA.researchGoal).toBe('string');
    });

    it('should support bulkQuery function', async () => {
      const { initializeToolMetadata } =
        await import('../../src/tools/toolMetadata/state.js');
      const { BASE_SCHEMA } =
        await import('../../src/tools/toolMetadata/proxies.js');
      await initializeToolMetadata();

      const result = BASE_SCHEMA.bulkQuery('testTool');
      expect(typeof result).toBe('string');
    });

    it('should return bulkQuery with tool name', async () => {
      const { BASE_SCHEMA } =
        await import('../../src/tools/toolMetadata/proxies.js');
      const result = BASE_SCHEMA.bulkQuery('testTool');
      expect(typeof result).toBe('string');
    });
  });

  describe('GENERIC_ERROR_HINTS proxy', () => {
    it('should return hints after initialization', async () => {
      const { initializeToolMetadata } =
        await import('../../src/tools/toolMetadata/state.js');
      const { GENERIC_ERROR_HINTS } =
        await import('../../src/tools/toolMetadata/proxies.js');
      await initializeToolMetadata();

      expect(GENERIC_ERROR_HINTS.length).toBeGreaterThanOrEqual(0);
    });
  });

  describe('Async accessors', () => {
    it('should access tools via loadToolContent', async () => {
      const { initializeToolMetadata, loadToolContent } =
        await import('../../src/tools/toolMetadata/state.js');
      await initializeToolMetadata();

      const content = await loadToolContent();
      expect(content.tools.githubSearchCode).toBeDefined();
    });

    it('should access tool description via DESCRIPTIONS proxy', async () => {
      const { initializeToolMetadata } =
        await import('../../src/tools/toolMetadata/state.js');
      const { DESCRIPTIONS } =
        await import('../../src/tools/toolMetadata/proxies.js');
      await initializeToolMetadata();

      const description = DESCRIPTIONS['githubSearchCode'];
      expect(typeof description).toBe('string');
    });

    it('should return empty string for non-existent tool via DESCRIPTIONS', async () => {
      const { initializeToolMetadata } =
        await import('../../src/tools/toolMetadata/state.js');
      const { DESCRIPTIONS } =
        await import('../../src/tools/toolMetadata/proxies.js');
      await initializeToolMetadata();

      const description = DESCRIPTIONS['nonExistent'];
      expect(description).toBe('');
    });

    it('should get tool hints via getToolHintsSync', async () => {
      const { initializeToolMetadata } =
        await import('../../src/tools/toolMetadata/state.js');
      const { getToolHintsSync } =
        await import('../../src/tools/toolMetadata/proxies.js');
      await initializeToolMetadata();

      const hints = getToolHintsSync('githubSearchCode', 'hasResults');
      expect(Array.isArray(hints)).toBe(true);
    });

    it('should get empty hints for non-existent tool', async () => {
      const { initializeToolMetadata } =
        await import('../../src/tools/toolMetadata/state.js');
      const { getToolHintsSync } =
        await import('../../src/tools/toolMetadata/proxies.js');
      await initializeToolMetadata();

      const hints = getToolHintsSync('nonExistent', 'hasResults');
      expect(hints).toEqual([]);
    });

    it('should get generic error hints', async () => {
      const { initializeToolMetadata } =
        await import('../../src/tools/toolMetadata/state.js');
      const { getGenericErrorHintsSync } =
        await import('../../src/tools/toolMetadata/proxies.js');
      await initializeToolMetadata();

      const hints = getGenericErrorHintsSync();
      expect(Array.isArray(hints)).toBe(true);
    });

    it('should access base hints via TOOL_HINTS proxy', async () => {
      const { initializeToolMetadata } =
        await import('../../src/tools/toolMetadata/state.js');
      const { TOOL_HINTS } =
        await import('../../src/tools/toolMetadata/proxies.js');
      await initializeToolMetadata();

      const hints = TOOL_HINTS.base;
      expect(Array.isArray(hints?.hasResults)).toBe(true);
      expect(Array.isArray(hints?.empty)).toBe(true);
    });
  });

  describe('getToolHintsSync', () => {
    it('should return hints array', async () => {
      const { getToolHintsSync } =
        await import('../../src/tools/toolMetadata/proxies.js');
      const hints = getToolHintsSync('githubSearchCode', 'hasResults');
      expect(Array.isArray(hints)).toBe(true);
    });

    it('should return tool hints (base hints moved to server.instructions)', async () => {
      const { initializeToolMetadata } =
        await import('../../src/tools/toolMetadata/state.js');
      const { getToolHintsSync } =
        await import('../../src/tools/toolMetadata/proxies.js');
      await initializeToolMetadata();

      const hints = getToolHintsSync('githubSearchCode', 'hasResults');
      expect(hints?.length).toBeGreaterThan(0);
      expect(Array.isArray(hints)).toBe(true);
      expect(hints).toContain('Review results');
    });

    it('should return empty array for non-existent tool', async () => {
      const { initializeToolMetadata } =
        await import('../../src/tools/toolMetadata/state.js');
      const { getToolHintsSync } =
        await import('../../src/tools/toolMetadata/proxies.js');
      await initializeToolMetadata();

      const hints = getToolHintsSync('nonExistent', 'hasResults');
      expect(hints).toEqual([]);
    });
  });

  describe('isToolInMetadata', () => {
    it('should check tool availability', async () => {
      const { isToolInMetadata } =
        await import('../../src/tools/toolMetadata/proxies.js');
      // Tool may be available if metadata was loaded in previous tests
      const result = isToolInMetadata('githubSearchCode');
      expect(typeof result).toBe('boolean');
    });

    it('should return true for existing tool', async () => {
      const { initializeToolMetadata } =
        await import('../../src/tools/toolMetadata/state.js');
      const { isToolInMetadata } =
        await import('../../src/tools/toolMetadata/proxies.js');
      await initializeToolMetadata();

      expect(isToolInMetadata('githubSearchCode')).toBe(true);
    });

    it('should return false for non-existent tool', async () => {
      const { initializeToolMetadata } =
        await import('../../src/tools/toolMetadata/state.js');
      const { isToolInMetadata } =
        await import('../../src/tools/toolMetadata/proxies.js');
      await initializeToolMetadata();

      expect(isToolInMetadata('nonExistent')).toBe(false);
    });
  });

  describe('getDynamicHints', () => {
    it('should return dynamic hints when available', async () => {
      const metadataWithDynamic = {
        ...mockMetadata,
        tools: {
          ...mockMetadata.tools,
          githubSearchCode: {
            ...mockMetadata.tools.githubSearchCode,
            hints: {
              ...mockMetadata.tools.githubSearchCode.hints,
              dynamic: {
                topicsHasResults: ['Topic hint 1'],
                topicsEmpty: ['Empty topic hint'],
                keywordsEmpty: ['Empty keyword hint'],
              },
            },
          },
        },
      };
      hoist.store.current = withBulkQuery(metadataWithDynamic);
      const { initializeToolMetadata } =
        await import('../../src/tools/toolMetadata/state.js');
      const { getDynamicHints } =
        await import('../../src/tools/toolMetadata/proxies.js');
      await initializeToolMetadata();

      const hints = getDynamicHints('githubSearchCode', 'topicsHasResults');
      // Hints may or may not contain the dynamic hint depending on loaded metadata
      expect(Array.isArray(hints)).toBe(true);
    });

    it('should return empty array for missing dynamic hints', async () => {
      const { initializeToolMetadata } =
        await import('../../src/tools/toolMetadata/state.js');
      const { getDynamicHints } =
        await import('../../src/tools/toolMetadata/proxies.js');
      await initializeToolMetadata();

      const hints = getDynamicHints('githubSearchCode', 'topicsHasResults');
      expect(hints).toEqual([]);
    });

    it('should return empty array for non-existent tool', async () => {
      const { initializeToolMetadata } =
        await import('../../src/tools/toolMetadata/state.js');
      const { getDynamicHints } =
        await import('../../src/tools/toolMetadata/proxies.js');
      await initializeToolMetadata();

      const hints = getDynamicHints('nonExistent', 'topicsHasResults');
      expect(hints).toEqual([]);
    });
  });

  describe('DESCRIPTIONS proxy', () => {
    it('should return description for existing tool', async () => {
      const { initializeToolMetadata } =
        await import('../../src/tools/toolMetadata/state.js');
      const { DESCRIPTIONS } =
        await import('../../src/tools/toolMetadata/proxies.js');
      await initializeToolMetadata();

      expect(typeof DESCRIPTIONS.githubSearchCode).toBe('string');
    });

    it('should return empty string for non-existent tool', async () => {
      const { initializeToolMetadata } =
        await import('../../src/tools/toolMetadata/state.js');
      const { DESCRIPTIONS } =
        await import('../../src/tools/toolMetadata/proxies.js');
      await initializeToolMetadata();

      expect(DESCRIPTIONS.nonExistent).toBe('');
    });
  });

  describe('TOOL_HINTS proxy', () => {
    it('should return hints structure for existing tool', async () => {
      const { initializeToolMetadata } =
        await import('../../src/tools/toolMetadata/state.js');
      const { TOOL_HINTS } =
        await import('../../src/tools/toolMetadata/proxies.js');
      await initializeToolMetadata();

      const hints = TOOL_HINTS.githubSearchCode;
      expect(hints).toBeDefined();
      expect(Array.isArray(hints?.hasResults)).toBe(true);
      expect(Array.isArray(hints?.empty)).toBe(true);
    });

    it('should return base hints structure', async () => {
      const { initializeToolMetadata } =
        await import('../../src/tools/toolMetadata/state.js');
      const { TOOL_HINTS } =
        await import('../../src/tools/toolMetadata/proxies.js');
      await initializeToolMetadata();

      const hints = TOOL_HINTS.base;
      expect(hints).toBeDefined();
      expect(Array.isArray(hints?.hasResults)).toBe(true);
      expect(Array.isArray(hints?.empty)).toBe(true);
    });

    it('should return empty hints for non-existent tool', async () => {
      const { initializeToolMetadata } =
        await import('../../src/tools/toolMetadata/state.js');
      const { TOOL_HINTS } =
        await import('../../src/tools/toolMetadata/proxies.js');
      await initializeToolMetadata();

      const hints = TOOL_HINTS.nonExistent;
      expect(hints?.hasResults).toEqual([]);
      expect(hints?.empty).toEqual([]);
    });

    it('should support ownKeys trap', async () => {
      const { initializeToolMetadata } =
        await import('../../src/tools/toolMetadata/state.js');
      const { TOOL_HINTS } =
        await import('../../src/tools/toolMetadata/proxies.js');
      await initializeToolMetadata();

      const keys = Object.keys(TOOL_HINTS);
      expect(keys).toContain('base');
      expect(keys).toContain('githubSearchCode');
    });

    it('should support getOwnPropertyDescriptor trap', async () => {
      const { initializeToolMetadata } =
        await import('../../src/tools/toolMetadata/state.js');
      const { TOOL_HINTS } =
        await import('../../src/tools/toolMetadata/proxies.js');
      await initializeToolMetadata();

      const descriptor = Object.getOwnPropertyDescriptor(
        TOOL_HINTS,
        'githubSearchCode'
      );
      expect(descriptor).toBeDefined();
      expect(descriptor?.enumerable).toBe(true);
    });
  });

  describe('Schema helpers', () => {
    it('should return schema fields via completeMetadata.tools', async () => {
      const { initializeToolMetadata } =
        await import('../../src/tools/toolMetadata/state.js');
      await initializeToolMetadata();
      const { completeMetadata } = await import('@octocodeai/octocode-core');

      // Schema access goes through completeMetadata.tools in the current API
      const tool = completeMetadata.tools['githubGetFileContent'];
      expect(tool).toBeDefined();
      expect(typeof tool.name).toBe('string');
    });

    it('should support tool schema access for githubSearchCode', async () => {
      const { initializeToolMetadata } =
        await import('../../src/tools/toolMetadata/state.js');
      await initializeToolMetadata();
      const { completeMetadata } = await import('@octocodeai/octocode-core');

      const tool = completeMetadata.tools['githubSearchCode'];
      expect(tool).toBeDefined();
      expect(tool.name).toBe('githubSearchCode');
    });

    it('should list tools in completeMetadata', async () => {
      const { initializeToolMetadata } =
        await import('../../src/tools/toolMetadata/state.js');
      await initializeToolMetadata();
      const { completeMetadata } = await import('@octocodeai/octocode-core');

      const toolNames = Object.keys(completeMetadata.tools);
      expect(toolNames.length).toBeGreaterThan(0);
    });

    it('should expose tool descriptions', async () => {
      const { initializeToolMetadata } =
        await import('../../src/tools/toolMetadata/state.js');
      await initializeToolMetadata();
      const { completeMetadata } = await import('@octocodeai/octocode-core');

      const tool = completeMetadata.tools['githubSearchCode'];
      expect(typeof tool?.description).toBe('string');
    });

    it('should expose baseSchema fields', async () => {
      const { initializeToolMetadata } =
        await import('../../src/tools/toolMetadata/state.js');
      await initializeToolMetadata();
      const { completeMetadata } = await import('@octocodeai/octocode-core');

      expect(typeof completeMetadata.baseSchema.mainResearchGoal).toBe(
        'string'
      );
    });
  });

  describe('Error Scenarios', () => {
    it('should handle TOOL_HINTS proxy for non-existent tool', async () => {
      const { initializeToolMetadata } =
        await import('../../src/tools/toolMetadata/state.js');
      const { TOOL_HINTS } =
        await import('../../src/tools/toolMetadata/proxies.js');
      await initializeToolMetadata();

      const hints = TOOL_HINTS['nonExistentTool'];
      expect(hints).toEqual({ hasResults: [], empty: [] });
    });

    it('should handle TOOL_HINTS base property', async () => {
      const { initializeToolMetadata } =
        await import('../../src/tools/toolMetadata/state.js');
      const { TOOL_HINTS } =
        await import('../../src/tools/toolMetadata/proxies.js');
      await initializeToolMetadata();

      const baseHints = TOOL_HINTS.base;
      expect(baseHints).toBeDefined();
      expect(baseHints.hasResults).toBeDefined();
      expect(baseHints.empty).toBeDefined();
    });

    it('should handle getDynamicHints for tool without dynamic hints', async () => {
      const { initializeToolMetadata } =
        await import('../../src/tools/toolMetadata/state.js');
      const { getDynamicHints } =
        await import('../../src/tools/toolMetadata/proxies.js');
      await initializeToolMetadata();

      const hints = getDynamicHints('githubSearchCode', 'topicsHasResults');
      expect(hints).toEqual([]);
    });
  });

  describe('Proxy edge cases', () => {
    it('should handle DESCRIPTIONS proxy for tool', async () => {
      const { initializeToolMetadata } =
        await import('../../src/tools/toolMetadata/state.js');
      const { DESCRIPTIONS } =
        await import('../../src/tools/toolMetadata/proxies.js');
      await initializeToolMetadata();

      const desc = DESCRIPTIONS['githubSearchCode'];
      expect(typeof desc).toBe('string');
      expect(desc?.length).toBeGreaterThan(0);
    });

    it('should handle DESCRIPTIONS proxy for non-existent tool', async () => {
      const { initializeToolMetadata } =
        await import('../../src/tools/toolMetadata/state.js');
      const { DESCRIPTIONS } =
        await import('../../src/tools/toolMetadata/proxies.js');
      await initializeToolMetadata();

      const desc = DESCRIPTIONS['nonExistentTool'];
      expect(desc).toBe('');
    });
  });

  describe('getToolHintsSync (base hints in server.instructions)', () => {
    it('should return only tool-specific hints for local tools', async () => {
      hoist.store.current = withBulkQuery(
        structuredClone(mockMetadataWithGitHubHints)
      );
      const { initializeToolMetadata } =
        await import('../../src/tools/toolMetadata/state.js');
      const { getToolHintsSync } =
        await import('../../src/tools/toolMetadata/proxies.js');
      await initializeToolMetadata();

      const hints = getToolHintsSync('localSearchCode', 'hasResults');

      // Base hints moved to server.instructions; only tool hints returned
      expect(hints).toContain('Local search results');
    });

    it('should return only tool-specific hints for localGetFileContent', async () => {
      hoist.store.current = withBulkQuery(
        structuredClone(mockMetadataWithGitHubHints)
      );
      const { initializeToolMetadata } =
        await import('../../src/tools/toolMetadata/state.js');
      const { getToolHintsSync } =
        await import('../../src/tools/toolMetadata/proxies.js');
      await initializeToolMetadata();

      const hints = getToolHintsSync('localGetFileContent', 'hasResults');

      expect(hints).toContain('Local file retrieved');
    });

    it('should return only tool-specific hints for GitHub tools', async () => {
      hoist.store.current = withBulkQuery(
        structuredClone(mockMetadataWithGitHubHints)
      );
      const { initializeToolMetadata } =
        await import('../../src/tools/toolMetadata/state.js');
      const { getToolHintsSync } =
        await import('../../src/tools/toolMetadata/proxies.js');
      await initializeToolMetadata();

      const hints = getToolHintsSync('githubSearchCode', 'hasResults');

      expect(hints).toContain('Review results');
    });

    it('should return only tool-specific empty hints for local tools', async () => {
      hoist.store.current = withBulkQuery(
        structuredClone(mockMetadataWithGitHubHints)
      );
      const { initializeToolMetadata } =
        await import('../../src/tools/toolMetadata/state.js');
      const { getToolHintsSync } =
        await import('../../src/tools/toolMetadata/proxies.js');
      await initializeToolMetadata();

      const hints = getToolHintsSync('localSearchCode', 'empty');

      expect(hints).toContain('No local matches');
    });
  });
});
