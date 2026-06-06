import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

describe('LSP Find References Tool', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.resetAllMocks();
  });

  describe('registerLSPFindReferencesTool', () => {
    it('should register tool with correct name and schema', async () => {
      vi.resetModules();

      const { registerLSPFindReferencesTool } =
        await import('../../src/tools/lsp_find_references/register.js');

      const mockServer = {
        registerTool: vi.fn().mockReturnValue(undefined),
      };

      registerLSPFindReferencesTool(mockServer as any);

      expect(mockServer.registerTool).toHaveBeenCalledWith(
        'lspFindReferences',
        expect.objectContaining({
          description: expect.any(String),
          inputSchema: expect.any(Object),
          annotations: expect.objectContaining({
            title: 'Find References',
            readOnlyHint: true,
            destructiveHint: false,
            idempotentHint: true,
          }),
        }),
        expect.any(Function)
      );
    });

    it('should have correct annotations', async () => {
      vi.resetModules();

      const { registerLSPFindReferencesTool } =
        await import('../../src/tools/lsp_find_references/register.js');

      const mockServer = {
        registerTool: vi.fn().mockReturnValue(undefined),
      };

      registerLSPFindReferencesTool(mockServer as any);

      const callArgs = mockServer.registerTool.mock.calls[0]!;
      const toolConfig = callArgs[1];

      expect(toolConfig.annotations.openWorldHint).toBe(false);
    });

    it('should register handler function', async () => {
      vi.resetModules();

      const { registerLSPFindReferencesTool } =
        await import('../../src/tools/lsp_find_references/register.js');

      const mockServer = {
        registerTool: vi.fn().mockReturnValue(undefined),
      };

      registerLSPFindReferencesTool(mockServer as any);

      const handler = mockServer.registerTool.mock.calls[0]![2];
      expect(typeof handler).toBe('function');
    });
  });

  describe('Schema validation', () => {
    it('should export correct schema', async () => {
      vi.resetModules();

      const { BulkLSPFindReferencesSchema } =
        await import('@octocodeai/octocode-core');

      expect(BulkLSPFindReferencesSchema).toBeDefined();
      const parsed = BulkLSPFindReferencesSchema.safeParse({
        queries: [
          {
            id: 'find_references_query',
            researchGoal: 'Find usages',
            reasoning: 'Trace references',
            uri: 'file:///test.ts',
            symbolName: 'testFn',
            lineHint: 1,
          },
        ],
      });
      expect(parsed.success).toBe(true);
    });

    it('should have queries property in schema', async () => {
      vi.resetModules();

      const { BulkLSPFindReferencesSchema } =
        await import('@octocodeai/octocode-core');

      const parsed = BulkLSPFindReferencesSchema.safeParse({
        queries: [
          {
            id: 'find_references_queries',
            researchGoal: 'Find usages',
            reasoning: 'Trace references',
            uri: 'file:///test.ts',
            symbolName: 'testFn',
            lineHint: 1,
          },
        ],
      });
      expect(parsed.success).toBe(true);
    });
  });

  describe('Tool name constant', () => {
    it('should use correct tool name from constants', async () => {
      vi.resetModules();

      const { STATIC_TOOL_NAMES } =
        await import('../../src/tools/toolNames.js');

      expect(STATIC_TOOL_NAMES.LSP_FIND_REFERENCES).toBeDefined();
      expect(typeof STATIC_TOOL_NAMES.LSP_FIND_REFERENCES).toBe('string');
    });
  });

  describe('Description export', () => {
    it('should export tool description', async () => {
      const { LSP_FIND_REFERENCES_DESCRIPTION } =
        await import('@octocodeai/octocode-core');

      expect(LSP_FIND_REFERENCES_DESCRIPTION).toBeDefined();
      expect(typeof LSP_FIND_REFERENCES_DESCRIPTION).toBe('string');
    });
  });

  describe('Reference sorting', () => {
    it('should sort definitions before usages', () => {
      const refs = [
        { uri: 'b.ts', isDefinition: false, range: { start: { line: 5 } } },
        { uri: 'a.ts', isDefinition: true, range: { start: { line: 1 } } },
        { uri: 'a.ts', isDefinition: false, range: { start: { line: 3 } } },
      ];

      refs.sort((a, b) => {
        if (a.isDefinition && !b.isDefinition) return -1;
        if (!a.isDefinition && b.isDefinition) return 1;
        if (a.uri !== b.uri) return a.uri.localeCompare(b.uri);
        return a.range.start.line - b.range.start.line;
      });

      expect(refs[0]!.isDefinition).toBe(true);
      expect(refs[1]!.uri).toBe('a.ts');
      expect(refs[2]!.uri).toBe('b.ts');
    });
  });
});
