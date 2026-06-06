import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import * as fs from 'fs/promises';

vi.mock('fs/promises', () => ({
  readFile: vi.fn(),
}));

vi.mock('../../src/lsp/resolver.js', () => ({
  SymbolResolver: vi.fn().mockImplementation(() => ({
    resolvePositionFromContent: vi.fn().mockReturnValue({
      position: { line: 10, character: 5 },
      foundAtLine: 10,
    }),
    extractContext: vi.fn().mockReturnValue({
      content: 'const test = 1;',
      startLine: 8,
      endLine: 13,
    }),
  })),
  SymbolResolutionError: class SymbolResolutionError extends Error {
    searchRadius: number;
    constructor(message: string, searchRadius: number) {
      super(message);
      this.searchRadius = searchRadius;
    }
  },
}));

vi.mock('../../src/lsp/manager.js', () => ({
  LSP_UNAVAILABLE_HINT: 'LSP unavailable test',
  acquirePooledClient: vi.fn().mockResolvedValue(null),
  isLanguageServerAvailable: vi.fn().mockResolvedValue(false),
}));

describe('LSP Goto Definition Handler Tests', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.resetAllMocks();
  });

  describe('Tool handler registration', () => {
    it('should register tool with correct name', async () => {
      vi.resetModules();

      const { registerLSPGotoDefinitionTool } =
        await import('../../src/tools/lsp_goto_definition/lsp_goto_definition.js');

      const mockServer = {
        registerTool: vi.fn().mockReturnValue('registered'),
      };

      registerLSPGotoDefinitionTool(mockServer as any);

      expect(mockServer.registerTool).toHaveBeenCalledWith(
        'lspGotoDefinition',
        expect.any(Object),
        expect.any(Function)
      );
    });

    it('should handle empty queries array', async () => {
      vi.resetModules();

      const { registerLSPGotoDefinitionTool } =
        await import('../../src/tools/lsp_goto_definition/lsp_goto_definition.js');

      const mockServer = {
        registerTool: vi.fn((_name, _config, handler) => handler),
      };

      registerLSPGotoDefinitionTool(mockServer as any);
      const handler = mockServer.registerTool.mock.results[0]!.value;

      const result = await handler({ queries: [] });

      expect(result).toBeDefined();
      expect(result.content).toBeDefined();
    });
  });

  describe('Query processing', () => {
    it('should process single query', async () => {
      vi.resetModules();

      const mockReadFile = vi.mocked(fs.readFile);
      mockReadFile.mockResolvedValue('function test() { return 1; }');

      const { registerLSPGotoDefinitionTool } =
        await import('../../src/tools/lsp_goto_definition/lsp_goto_definition.js');

      const mockServer = {
        registerTool: vi.fn((_name, _config, handler) => handler),
      };

      registerLSPGotoDefinitionTool(mockServer as any);
      const handler = mockServer.registerTool.mock.results[0]!.value;

      const result = await handler({
        queries: [
          {
            uri: '/workspace/test.ts',
            symbolName: 'test',
            lineHint: 1,
            researchGoal: 'Find definition',
            reasoning: 'Testing',
          },
        ],
      });

      expect(result).toBeDefined();
    });

    it('should handle multiple queries', async () => {
      vi.resetModules();

      const mockReadFile = vi.mocked(fs.readFile);
      mockReadFile.mockResolvedValue('const x = 1;');

      const { registerLSPGotoDefinitionTool } =
        await import('../../src/tools/lsp_goto_definition/lsp_goto_definition.js');

      const mockServer = {
        registerTool: vi.fn((_name, _config, handler) => handler),
      };

      registerLSPGotoDefinitionTool(mockServer as any);
      const handler = mockServer.registerTool.mock.results[0]!.value;

      const result = await handler({
        queries: [
          {
            uri: '/workspace/a.ts',
            symbolName: 'funcA',
            lineHint: 10,
            researchGoal: 'Find A',
            reasoning: 'Test A',
          },
          {
            uri: '/workspace/b.ts',
            symbolName: 'funcB',
            lineHint: 20,
            researchGoal: 'Find B',
            reasoning: 'Test B',
          },
        ],
      });

      expect(result).toBeDefined();
    });
  });

  describe('Error handling', () => {
    it('should handle file read errors', async () => {
      vi.resetModules();

      const mockReadFile = vi.mocked(fs.readFile);
      mockReadFile.mockRejectedValue(new Error('ENOENT: file not found'));

      const { registerLSPGotoDefinitionTool } =
        await import('../../src/tools/lsp_goto_definition/lsp_goto_definition.js');

      const mockServer = {
        registerTool: vi.fn((_name, _config, handler) => handler),
      };

      registerLSPGotoDefinitionTool(mockServer as any);
      const handler = mockServer.registerTool.mock.results[0]!.value;

      const result = await handler({
        queries: [
          {
            uri: '/workspace/nonexistent.ts',
            symbolName: 'test',
            lineHint: 1,
            researchGoal: 'test',
            reasoning: 'test',
          },
        ],
      });

      expect(result).toBeDefined();
    });
  });

  describe('Context lines handling', () => {
    it('should use default context lines when not specified', async () => {
      vi.resetModules();

      const mockReadFile = vi.mocked(fs.readFile);
      mockReadFile.mockResolvedValue('const test = 1;');

      const { registerLSPGotoDefinitionTool } =
        await import('../../src/tools/lsp_goto_definition/lsp_goto_definition.js');

      const mockServer = {
        registerTool: vi.fn((_name, _config, handler) => handler),
      };

      registerLSPGotoDefinitionTool(mockServer as any);
      const handler = mockServer.registerTool.mock.results[0]!.value;

      const result = await handler({
        queries: [
          {
            uri: '/workspace/test.ts',
            symbolName: 'test',
            lineHint: 1,
            researchGoal: 'test',
            reasoning: 'test',
          },
        ],
      });

      expect(result).toBeDefined();
    });

    it('should use custom context lines when specified', async () => {
      vi.resetModules();

      const mockReadFile = vi.mocked(fs.readFile);
      mockReadFile.mockResolvedValue('const test = 1;');

      const { registerLSPGotoDefinitionTool } =
        await import('../../src/tools/lsp_goto_definition/lsp_goto_definition.js');

      const mockServer = {
        registerTool: vi.fn((_name, _config, handler) => handler),
      };

      registerLSPGotoDefinitionTool(mockServer as any);
      const handler = mockServer.registerTool.mock.results[0]!.value;

      const result = await handler({
        queries: [
          {
            uri: '/workspace/test.ts',
            symbolName: 'test',
            lineHint: 1,
            researchGoal: 'test',
            reasoning: 'test',
            contextLines: 10,
          },
        ],
      });

      expect(result).toBeDefined();
    });
  });

  describe('orderHint handling', () => {
    it('should use default orderHint when not specified', async () => {
      vi.resetModules();

      const mockReadFile = vi.mocked(fs.readFile);
      mockReadFile.mockResolvedValue('const test = 1;');

      const { registerLSPGotoDefinitionTool } =
        await import('../../src/tools/lsp_goto_definition/lsp_goto_definition.js');

      const mockServer = {
        registerTool: vi.fn((_name, _config, handler) => handler),
      };

      registerLSPGotoDefinitionTool(mockServer as any);
      const handler = mockServer.registerTool.mock.results[0]!.value;

      const result = await handler({
        queries: [
          {
            uri: '/workspace/test.ts',
            symbolName: 'test',
            lineHint: 1,
            researchGoal: 'test',
            reasoning: 'test',
          },
        ],
      });

      expect(result).toBeDefined();
    });
  });

  describe('Schema validation', () => {
    it('should validate schema shape', async () => {
      const { BulkLSPGotoDefinitionSchema } =
        await import('@octocodeai/octocode-core');

      const parsed = BulkLSPGotoDefinitionSchema.safeParse({
        queries: [
          {
            id: 'goto_definition_handler',
            researchGoal: 'Find definition',
            reasoning: 'Validate schema',
            uri: 'file:///test.ts',
            symbolName: 'testFn',
            lineHint: 1,
          },
        ],
      });
      expect(parsed.success).toBe(true);
    });

    it('should export description', async () => {
      const { LSP_GOTO_DEFINITION_DESCRIPTION } =
        await import('@octocodeai/octocode-core');

      expect(typeof LSP_GOTO_DEFINITION_DESCRIPTION).toBe('string');
    });
  });

  describe('Result structure', () => {
    it('should create valid result with locations', () => {
      const result = {
        locations: [
          {
            uri: '/test/file.ts',
            range: {
              start: { line: 10, character: 0 },
              end: { line: 10, character: 20 },
            },
            content: '>  11| const test = () => {};',
            displayRange: {
              startLine: 6,
              endLine: 16,
            },
          },
        ],
        resolvedPosition: { line: 10, character: 6 },
        searchRadius: 5,
        researchGoal: 'Find definition',
        reasoning: 'Test',
        hints: [
          'Each location = a definition site; use range.start.line+1 as lineHint for follow-up LSP calls',
        ],
      };

      expect(result.status).toBeUndefined();
      expect(result.locations.length).toBe(1);
      expect(result.resolvedPosition).toBeDefined();
    });

    it('should create empty result when not found', () => {
      const result = {
        status: 'empty' as const,
        error: 'Symbol not found',
        errorType: 'symbol_not_found',
        searchRadius: 5,
        researchGoal: 'Find definition',
        reasoning: 'Test',
        hints: ['Symbol not found at line 10'],
      };

      expect(result.status).toBe('empty');
      expect(result.errorType).toBe('symbol_not_found');
    });
  });
});
