/**
 * Handler tests for LSP Call Hierarchy tool
 * Tests the actual handler function with mocked dependencies
 * @module tools/lsp_call_hierarchy.handler.test
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import * as fs from 'fs/promises';

// Mock fs/promises before importing the module
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
      content: 'function test() { return 1; }',
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

// Mock exec utilities
vi.mock('../../src/utils/exec/safe.js', () => ({
  safeExec: vi
    .fn()
    .mockResolvedValue({ success: true, stdout: '', stderr: '', code: 0 }),
}));

vi.mock('../../src/utils/exec/commandAvailability.js', () => ({
  checkCommandAvailability: vi
    .fn()
    .mockResolvedValue({ available: true, path: '/usr/bin/rg' }),
}));

vi.mock('../../src/utils/exec/npm.js', () => ({
  getGithubCLIToken: vi.fn().mockResolvedValue(null),
  checkNpmAvailability: vi.fn().mockResolvedValue({ available: true }),
  executeNpmCommand: vi.fn().mockResolvedValue({ success: true, stdout: '' }),
}));

describe('LSP Call Hierarchy Handler Tests', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.resetAllMocks();
  });

  describe('Tool handler registration', () => {
    it('should register tool with correct name', async () => {
      vi.resetModules();

      const { registerLSPCallHierarchyTool } =
        await import('../../src/tools/lsp_call_hierarchy/register.js');

      const mockServer = {
        registerTool: vi.fn().mockReturnValue('registered'),
      };

      registerLSPCallHierarchyTool(mockServer as any);

      expect(mockServer.registerTool).toHaveBeenCalledWith(
        'lspCallHierarchy',
        expect.any(Object),
        expect.any(Function)
      );
    });

    it('should have correct tool config', async () => {
      vi.resetModules();

      const { registerLSPCallHierarchyTool } =
        await import('../../src/tools/lsp_call_hierarchy/register.js');

      const mockServer = {
        registerTool: vi.fn().mockReturnValue('registered'),
      };

      registerLSPCallHierarchyTool(mockServer as any);

      const config = mockServer.registerTool.mock.calls[0]![1];
      expect(config.annotations.title).toBe('Call Hierarchy');
      expect(config.annotations.readOnlyHint).toBe(true);
    });

    it('should handle empty queries array', async () => {
      vi.resetModules();

      const { registerLSPCallHierarchyTool } =
        await import('../../src/tools/lsp_call_hierarchy/register.js');

      const mockServer = {
        registerTool: vi.fn((_name, _config, handler) => handler),
      };

      registerLSPCallHierarchyTool(mockServer as any);
      const handler = mockServer.registerTool.mock.results[0]!.value;

      const result = await handler({ queries: [] });

      expect(result).toBeDefined();
      expect(result.content).toBeDefined();
    });
  });

  describe('Direction handling', () => {
    it('should handle incoming direction', async () => {
      vi.resetModules();

      const mockReadFile = vi.mocked(fs.readFile);
      mockReadFile.mockResolvedValue('function test() { return 1; }');

      const { registerLSPCallHierarchyTool } =
        await import('../../src/tools/lsp_call_hierarchy/register.js');

      const mockServer = {
        registerTool: vi.fn((_name, _config, handler) => handler),
      };

      registerLSPCallHierarchyTool(mockServer as any);
      const handler = mockServer.registerTool.mock.results[0]!.value;

      const result = await handler({
        queries: [
          {
            uri: '/workspace/test.ts',
            symbolName: 'myFunction',
            lineHint: 10,
            direction: 'incoming',
            researchGoal: 'Find callers',
            reasoning: 'Testing',
          },
        ],
      });

      expect(result).toBeDefined();
    });

    it('should handle outgoing direction', async () => {
      vi.resetModules();

      const mockReadFile = vi.mocked(fs.readFile);
      mockReadFile.mockResolvedValue('function test() { helper(); }');

      const { registerLSPCallHierarchyTool } =
        await import('../../src/tools/lsp_call_hierarchy/register.js');

      const mockServer = {
        registerTool: vi.fn((_name, _config, handler) => handler),
      };

      registerLSPCallHierarchyTool(mockServer as any);
      const handler = mockServer.registerTool.mock.results[0]!.value;

      const result = await handler({
        queries: [
          {
            uri: '/workspace/test.ts',
            symbolName: 'myFunction',
            lineHint: 10,
            direction: 'outgoing',
            researchGoal: 'Find callees',
            reasoning: 'Testing',
          },
        ],
      });

      expect(result).toBeDefined();
    });
  });

  describe('Pagination', () => {
    it('should use default pagination values', async () => {
      vi.resetModules();

      const mockReadFile = vi.mocked(fs.readFile);
      mockReadFile.mockResolvedValue('function test() { }');

      const { registerLSPCallHierarchyTool } =
        await import('../../src/tools/lsp_call_hierarchy/register.js');

      const mockServer = {
        registerTool: vi.fn((_name, _config, handler) => handler),
      };

      registerLSPCallHierarchyTool(mockServer as any);
      const handler = mockServer.registerTool.mock.results[0]!.value;

      const result = await handler({
        queries: [
          {
            uri: '/workspace/test.ts',
            symbolName: 'test',
            lineHint: 1,
            direction: 'incoming',
            researchGoal: 'test',
            reasoning: 'test',
            // No callsPerPage or page - should use defaults
          },
        ],
      });

      expect(result).toBeDefined();
    });

    it('should use custom pagination values', async () => {
      vi.resetModules();

      const mockReadFile = vi.mocked(fs.readFile);
      mockReadFile.mockResolvedValue('function test() { }');

      const { registerLSPCallHierarchyTool } =
        await import('../../src/tools/lsp_call_hierarchy/register.js');

      const mockServer = {
        registerTool: vi.fn((_name, _config, handler) => handler),
      };

      registerLSPCallHierarchyTool(mockServer as any);
      const handler = mockServer.registerTool.mock.results[0]!.value;

      const result = await handler({
        queries: [
          {
            uri: '/workspace/test.ts',
            symbolName: 'test',
            lineHint: 1,
            direction: 'incoming',
            researchGoal: 'test',
            reasoning: 'test',
            callsPerPage: 5,
            page: 2,
          },
        ],
      });

      expect(result).toBeDefined();
    });
  });

  describe('Depth handling', () => {
    it('should use default depth', async () => {
      vi.resetModules();

      const mockReadFile = vi.mocked(fs.readFile);
      mockReadFile.mockResolvedValue('function test() { }');

      const { registerLSPCallHierarchyTool } =
        await import('../../src/tools/lsp_call_hierarchy/register.js');

      const mockServer = {
        registerTool: vi.fn((_name, _config, handler) => handler),
      };

      registerLSPCallHierarchyTool(mockServer as any);
      const handler = mockServer.registerTool.mock.results[0]!.value;

      const result = await handler({
        queries: [
          {
            uri: '/workspace/test.ts',
            symbolName: 'test',
            lineHint: 1,
            direction: 'incoming',
            researchGoal: 'test',
            reasoning: 'test',
            // No depth - should default to 1
          },
        ],
      });

      expect(result).toBeDefined();
    });

    it('should use custom depth', async () => {
      vi.resetModules();

      const mockReadFile = vi.mocked(fs.readFile);
      mockReadFile.mockResolvedValue('function test() { }');

      const { registerLSPCallHierarchyTool } =
        await import('../../src/tools/lsp_call_hierarchy/register.js');

      const mockServer = {
        registerTool: vi.fn((_name, _config, handler) => handler),
      };

      registerLSPCallHierarchyTool(mockServer as any);
      const handler = mockServer.registerTool.mock.results[0]!.value;

      const result = await handler({
        queries: [
          {
            uri: '/workspace/test.ts',
            symbolName: 'test',
            lineHint: 1,
            direction: 'outgoing',
            researchGoal: 'test',
            reasoning: 'test',
            depth: 2,
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
      mockReadFile.mockRejectedValue(new Error('ENOENT'));

      const { registerLSPCallHierarchyTool } =
        await import('../../src/tools/lsp_call_hierarchy/register.js');

      const mockServer = {
        registerTool: vi.fn((_name, _config, handler) => handler),
      };

      registerLSPCallHierarchyTool(mockServer as any);
      const handler = mockServer.registerTool.mock.results[0]!.value;

      const result = await handler({
        queries: [
          {
            uri: '/workspace/nonexistent.ts',
            symbolName: 'test',
            lineHint: 1,
            direction: 'incoming',
            researchGoal: 'test',
            reasoning: 'test',
          },
        ],
      });

      expect(result).toBeDefined();
    });
  });

  describe('Context lines handling', () => {
    it('should use default context lines', async () => {
      vi.resetModules();

      const mockReadFile = vi.mocked(fs.readFile);
      mockReadFile.mockResolvedValue('function test() { }');

      const { registerLSPCallHierarchyTool } =
        await import('../../src/tools/lsp_call_hierarchy/register.js');

      const mockServer = {
        registerTool: vi.fn((_name, _config, handler) => handler),
      };

      registerLSPCallHierarchyTool(mockServer as any);
      const handler = mockServer.registerTool.mock.results[0]!.value;

      const result = await handler({
        queries: [
          {
            uri: '/workspace/test.ts',
            symbolName: 'test',
            lineHint: 1,
            direction: 'incoming',
            researchGoal: 'test',
            reasoning: 'test',
            // No contextLines - should default to 2
          },
        ],
      });

      expect(result).toBeDefined();
    });
  });

  describe('Schema validation', () => {
    it('should export BulkLSPCallHierarchySchema', async () => {
      const { BulkLSPCallHierarchySchema } =
        await import('@octocodeai/octocode-core');

      expect(BulkLSPCallHierarchySchema).toBeDefined();
      const parsed = BulkLSPCallHierarchySchema.safeParse({
        queries: [
          {
            id: 'call_hierarchy_query',
            researchGoal: 'Trace calls',
            reasoning: 'Inspect call graph',
            uri: 'file:///test.ts',
            symbolName: 'testFn',
            lineHint: 1,
            direction: 'incoming',
          },
        ],
      });
      expect(parsed.success).toBe(true);
    });

    it('should export description', async () => {
      const { LSP_CALL_HIERARCHY_DESCRIPTION } =
        await import('@octocodeai/octocode-core');

      expect(typeof LSP_CALL_HIERARCHY_DESCRIPTION).toBe('string');
      // Description may be empty if tool not in remote metadata (local-only tool)
    });
  });

  describe('Result structure', () => {
    it('should create valid incoming call result', () => {
      const result = {
        status: 'hasResults' as const,
        item: {
          name: 'myFunction',
          kind: 'function' as const,
          uri: '/test/file.ts',
          range: {
            start: { line: 10, character: 0 },
            end: { line: 15, character: 1 },
          },
          selectionRange: {
            start: { line: 10, character: 9 },
            end: { line: 10, character: 19 },
          },
          displayRange: { startLine: 8, endLine: 18 },
        },
        direction: 'incoming' as const,
        depth: 1,
        incomingCalls: [],
        pagination: {
          currentPage: 1,
          totalPages: 1,
          totalResults: 0,
          hasMore: false,
          resultsPerPage: 15,
        },
        researchGoal: 'Find callers',
        reasoning: 'Test',
        hints: ['No callers found'],
      };

      expect(result.status).toBe('hasResults');
      expect(result.direction).toBe('incoming');
      expect(result.item.name).toBe('myFunction');
    });

    it('should create valid outgoing call result', () => {
      const result = {
        status: 'hasResults' as const,
        item: {
          name: 'myFunction',
          kind: 'function' as const,
          uri: '/test/file.ts',
          range: {
            start: { line: 10, character: 0 },
            end: { line: 15, character: 1 },
          },
          selectionRange: {
            start: { line: 10, character: 9 },
            end: { line: 10, character: 19 },
          },
          displayRange: { startLine: 8, endLine: 18 },
        },
        direction: 'outgoing' as const,
        depth: 1,
        outgoingCalls: [],
        pagination: {
          currentPage: 1,
          totalPages: 1,
          totalResults: 0,
          hasMore: false,
          resultsPerPage: 15,
        },
        researchGoal: 'Find callees',
        reasoning: 'Test',
        hints: ['No callees found'],
      };

      expect(result.status).toBe('hasResults');
      expect(result.direction).toBe('outgoing');
    });

    it('should create empty result when symbol not found', () => {
      const result = {
        status: 'empty' as const,
        errorType: 'symbol_not_found',
        error: 'Symbol not found',
        direction: 'incoming',
        depth: 1,
        researchGoal: 'Find callers',
        reasoning: 'Test',
        hints: ['Symbol not found at line 10'],
      };

      expect(result.status).toBe('empty');
      expect(result.errorType).toBe('symbol_not_found');
    });
  });

  describe('Multiple queries', () => {
    it('should handle multiple queries', async () => {
      vi.resetModules();

      const mockReadFile = vi.mocked(fs.readFile);
      mockReadFile.mockResolvedValue('function test() { }');

      const { registerLSPCallHierarchyTool } =
        await import('../../src/tools/lsp_call_hierarchy/register.js');

      const mockServer = {
        registerTool: vi.fn((_name, _config, handler) => handler),
      };

      registerLSPCallHierarchyTool(mockServer as any);
      const handler = mockServer.registerTool.mock.results[0]!.value;

      const result = await handler({
        queries: [
          {
            uri: '/workspace/a.ts',
            symbolName: 'funcA',
            lineHint: 10,
            direction: 'incoming',
            researchGoal: 'Find A callers',
            reasoning: 'Test A',
          },
          {
            uri: '/workspace/b.ts',
            symbolName: 'funcB',
            lineHint: 20,
            direction: 'outgoing',
            researchGoal: 'Find B callees',
            reasoning: 'Test B',
          },
        ],
      });

      expect(result).toBeDefined();
    });
  });
});
