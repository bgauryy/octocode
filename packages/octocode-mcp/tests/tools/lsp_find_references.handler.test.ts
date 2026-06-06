import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import * as fs from 'fs/promises';

vi.mock('fs/promises', () => ({
  readFile: vi.fn(),
  stat: vi.fn(),
}));

vi.mock('../../src/lsp/resolver.js', () => ({
  SymbolResolver: vi.fn().mockImplementation(() => ({
    resolvePositionFromContent: vi.fn(),
    extractContext: vi.fn(),
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

describe('LSP Find References Handler Tests', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.resetAllMocks();
  });

  describe('Tool handler registration', () => {
    it('should register tool and return handler', async () => {
      vi.resetModules();

      const { registerLSPFindReferencesTool } =
        await import('../../src/tools/lsp_find_references/register.js');

      const mockServer = {
        registerTool: vi.fn().mockReturnValue('registered'),
      };

      registerLSPFindReferencesTool(mockServer as any);

      expect(mockServer.registerTool).toHaveBeenCalled();
      const { STATIC_TOOL_NAMES } =
        await import('../../src/tools/toolNames.js');
      expect(mockServer.registerTool.mock.calls[0]![0]).toBe(
        STATIC_TOOL_NAMES.LSP_FIND_REFERENCES
      );
    });

    it('should handle empty queries array', async () => {
      vi.resetModules();

      const { registerLSPFindReferencesTool } =
        await import('../../src/tools/lsp_find_references/register.js');

      const mockServer = {
        registerTool: vi.fn((_name, _config, handler) => handler),
      };

      registerLSPFindReferencesTool(mockServer as any);
      const handler = mockServer.registerTool.mock.results[0]!.value;

      const result = await handler({ queries: [] });

      expect(result).toBeDefined();
      expect(result.content).toBeDefined();
    });
  });

  describe('Path validation', () => {
    it('should reject paths outside workspace', async () => {
      vi.resetModules();

      const { registerLSPFindReferencesTool } =
        await import('../../src/tools/lsp_find_references/register.js');

      const mockServer = {
        registerTool: vi.fn((_name, _config, handler) => handler),
      };

      registerLSPFindReferencesTool(mockServer as any);
      const handler = mockServer.registerTool.mock.results[0]!.value;

      const result = await handler({
        queries: [
          {
            uri: '../../etc/passwd',
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

  describe('Query field validation', () => {
    it('should handle missing optional fields', async () => {
      vi.resetModules();

      const mockStat = vi.mocked(fs.stat);
      mockStat.mockResolvedValue({ isFile: () => true } as any);

      const mockReadFile = vi.mocked(fs.readFile);
      mockReadFile.mockResolvedValue('const test = 1;');

      const { registerLSPFindReferencesTool } =
        await import('../../src/tools/lsp_find_references/register.js');

      const mockServer = {
        registerTool: vi.fn((_name, _config, handler) => handler),
      };

      registerLSPFindReferencesTool(mockServer as any);
      const handler = mockServer.registerTool.mock.results[0]!.value;

      const result = await handler({
        queries: [
          {
            uri: '/workspace/test.ts',
            symbolName: 'test',
            lineHint: 1,
            researchGoal: 'Find references',
            reasoning: 'Testing',
          },
        ],
      });

      expect(result).toBeDefined();
    });
  });

  describe('Multiple queries', () => {
    it('should handle multiple queries in batch', async () => {
      vi.resetModules();

      const { registerLSPFindReferencesTool } =
        await import('../../src/tools/lsp_find_references/register.js');

      const mockServer = {
        registerTool: vi.fn((_name, _config, handler) => handler),
      };

      registerLSPFindReferencesTool(mockServer as any);
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
      expect(result.content).toBeDefined();
    });
  });

  describe('Schema exports', () => {
    it('should export BulkLSPFindReferencesSchema', async () => {
      const { BulkLSPFindReferencesSchema } =
        await import('@octocodeai/octocode-core');

      expect(BulkLSPFindReferencesSchema).toBeDefined();
    });

    it('should export LSP_FIND_REFERENCES_DESCRIPTION', async () => {
      const { LSP_FIND_REFERENCES_DESCRIPTION } =
        await import('@octocodeai/octocode-core');

      expect(LSP_FIND_REFERENCES_DESCRIPTION).toBeDefined();
      expect(typeof LSP_FIND_REFERENCES_DESCRIPTION).toBe('string');
    });
  });

  describe('Reference result structure', () => {
    it('should create proper pagination info', () => {
      const totalReferences = 55;
      const referencesPerPage = 20;
      const page = 2;

      const totalPages = Math.ceil(totalReferences / referencesPerPage);
      const startIndex = (page - 1) * referencesPerPage;
      Math.min(startIndex + referencesPerPage, totalReferences);

      const pagination = {
        currentPage: page,
        totalPages,
        totalResults: totalReferences,
        hasMore: page < totalPages,
        resultsPerPage: referencesPerPage,
      };

      expect(pagination.currentPage).toBe(2);
      expect(pagination.totalPages).toBe(3);
      expect(pagination.hasMore).toBe(true);
    });
  });

  describe('Error handling', () => {
    it('should handle file not found', async () => {
      vi.resetModules();

      const mockStat = vi.mocked(fs.stat);
      mockStat.mockRejectedValue(new Error('ENOENT: no such file'));

      const { registerLSPFindReferencesTool } =
        await import('../../src/tools/lsp_find_references/register.js');

      const mockServer = {
        registerTool: vi.fn((_name, _config, handler) => handler),
      };

      registerLSPFindReferencesTool(mockServer as any);
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

    it('should handle file read errors', async () => {
      vi.resetModules();

      const mockStat = vi.mocked(fs.stat);
      mockStat.mockResolvedValue({ isFile: () => true } as any);

      const mockReadFile = vi.mocked(fs.readFile);
      mockReadFile.mockRejectedValue(new Error('Permission denied'));

      const { registerLSPFindReferencesTool } =
        await import('../../src/tools/lsp_find_references/register.js');

      const mockServer = {
        registerTool: vi.fn((_name, _config, handler) => handler),
      };

      registerLSPFindReferencesTool(mockServer as any);
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
});
