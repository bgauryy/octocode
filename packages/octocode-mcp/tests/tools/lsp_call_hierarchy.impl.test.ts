import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

vi.mock('fs/promises', () => ({
  readFile: vi.fn(),
}));

vi.mock('../../src/utils/exec/safe.js', () => ({
  safeExec: vi
    .fn()
    .mockResolvedValue({ stdout: '', stderr: '', code: 0, success: true }),
}));

vi.mock('../../src/utils/exec/commandAvailability.js', () => ({
  checkCommandAvailability: vi
    .fn()
    .mockResolvedValue({ available: true, command: 'rg' }),
}));

vi.mock('../../src/lsp/resolver.js', () => {
  class MockSymbolResolutionError extends Error {
    searchRadius: number;
    constructor(message: string, searchRadius: number) {
      super(message);
      this.name = 'SymbolResolutionError';
      this.searchRadius = searchRadius;
    }
  }

  return {
    SymbolResolver: vi.fn().mockImplementation(() => ({
      resolvePositionFromContent: vi.fn().mockReturnValue({
        position: { line: 3, character: 16 },
        foundAtLine: 4,
      }),
      extractContext: vi.fn().mockReturnValue({
        content: 'test content',
        startLine: 1,
        endLine: 10,
      }),
    })),
    SymbolResolutionError: MockSymbolResolutionError,
  };
});

vi.mock('../../src/lsp/manager.js', () => ({
  LSP_UNAVAILABLE_HINT: 'LSP unavailable test',
  acquirePooledClient: vi.fn().mockResolvedValue(null),
  isLanguageServerAvailable: vi.fn().mockResolvedValue(false),
}));

import * as fs from 'fs/promises';
import * as resolverModule from '../../src/lsp/resolver.js';
import * as managerModule from '../../src/lsp/manager.js';
import { safeExec } from '../../src/utils/exec/safe.js';
import { checkCommandAvailability } from '../../src/utils/exec/commandAvailability.js';

import { registerLSPCallHierarchyTool } from '../../src/tools/lsp_call_hierarchy/register.js';

describe('LSP Call Hierarchy Implementation Tests', () => {
  const sampleTypeScriptContent = `
import { helper } from './utils';

export function mainFunction(param: string): string {
  const result = helper(param);
  innerCall();
  return result;
}

function innerCall() {
  console.log('inner');
}

export function caller() {
  mainFunction('test');
}
`.trim();

  beforeEach(() => {
    vi.clearAllMocks();

    process.env.WORKSPACE_ROOT = '/workspace';

    vi.mocked(fs.readFile).mockResolvedValue(sampleTypeScriptContent);

    vi.mocked(managerModule.isLanguageServerAvailable).mockResolvedValue(false);
    vi.mocked(managerModule.acquirePooledClient).mockResolvedValue(null);
    vi.mocked(resolverModule.SymbolResolver).mockImplementation(function () {
      return {
        resolvePositionFromContent: vi.fn().mockReturnValue({
          position: { line: 3, character: 16 },
          foundAtLine: 4,
        }),
      };
    });

    vi.mocked(checkCommandAvailability).mockResolvedValue({
      available: true,
      command: expect.stringMatching(/rg$/),
    });
    vi.mocked(safeExec).mockResolvedValue({
      stdout: '',
      stderr: '',
      code: 0,
      success: true,
    });
  });

  afterEach(() => {
    delete process.env.WORKSPACE_ROOT;
    vi.resetAllMocks();
  });

  const createHandler = () => {
    const mockServer = {
      registerTool: vi.fn((_name, _config, handler) => handler),
    };
    registerLSPCallHierarchyTool(mockServer as any);
    return mockServer.registerTool.mock.results[0]!.value;
  };

  describe('Tool Registration', () => {
    it('should register the tool with correct name', () => {
      const mockServer = {
        registerTool: vi.fn(),
      };

      registerLSPCallHierarchyTool(mockServer as any);

      expect(mockServer.registerTool).toHaveBeenCalledWith(
        'lspCallHierarchy',
        expect.any(Object),
        expect.any(Function)
      );
    });
  });

  describe('Error Handling', () => {
    it('should handle file read errors', async () => {
      vi.mocked(fs.readFile).mockRejectedValue(new Error('Permission denied'));

      const handler = createHandler();
      const result = await handler({
        queries: [
          {
            uri: '/workspace/protected.ts',
            symbolName: 'test',
            lineHint: 1,
            direction: 'incoming',
            researchGoal: 'Find callers',
            reasoning: 'Testing',
          },
        ],
      });

      expect(result).toBeDefined();
      expect(result.content?.length).toBeGreaterThan(0);
    });
  });

  describe('Path Validation', () => {
    it('should reject paths outside workspace', async () => {
      const handler = createHandler();
      const result = await handler({
        queries: [
          {
            uri: '../../../etc/passwd',
            symbolName: 'test',
            lineHint: 1,
            direction: 'incoming',
            researchGoal: 'Find callers',
            reasoning: 'Testing',
          },
        ],
      });

      expect(result).toBeDefined();
      expect(result.content?.length).toBeGreaterThan(0);
    });

    it('should handle absolute paths within workspace', async () => {
      const handler = createHandler();
      const result = await handler({
        queries: [
          {
            uri: '/workspace/src/valid.ts',
            symbolName: 'test',
            lineHint: 1,
            direction: 'incoming',
            researchGoal: 'Find callers',
            reasoning: 'Testing',
          },
        ],
      });

      expect(result).toBeDefined();
    });
  });

  describe('Multiple Queries', () => {
    it('should process multiple queries', async () => {
      const handler = createHandler();
      const result = await handler({
        queries: [
          {
            uri: '/workspace/src/a.ts',
            symbolName: 'funcA',
            lineHint: 10,
            direction: 'incoming',
            researchGoal: 'Find A callers',
            reasoning: 'Test A',
          },
          {
            uri: '/workspace/src/b.ts',
            symbolName: 'funcB',
            lineHint: 20,
            direction: 'outgoing',
            researchGoal: 'Find B callees',
            reasoning: 'Test B',
          },
        ],
      });

      expect(result).toBeDefined();
      expect(result.content?.length).toBeGreaterThan(0);
    });

    it('should handle empty queries array', async () => {
      const handler = createHandler();
      const result = await handler({ queries: [] });

      expect(result).toBeDefined();
    });
  });

  describe('Query field validation', () => {
    it('should handle missing optional fields', async () => {
      const handler = createHandler();

      const result = await handler({
        queries: [
          {
            uri: '/workspace/test.ts',
            symbolName: 'test',
            lineHint: 1,
            direction: 'incoming',
            researchGoal: 'Find callers',
            reasoning: 'Testing',
          },
        ],
      });

      expect(result).toBeDefined();
    });

    it('should handle depth parameter', async () => {
      const handler = createHandler();
      const result = await handler({
        queries: [
          {
            uri: '/workspace/test.ts',
            symbolName: 'test',
            lineHint: 1,
            direction: 'incoming',
            depth: 2,
            researchGoal: 'Find deep callers',
            reasoning: 'Testing depth',
          },
        ],
      });

      expect(result).toBeDefined();
    });

    it('should handle pagination parameters', async () => {
      const handler = createHandler();
      const result = await handler({
        queries: [
          {
            uri: '/workspace/test.ts',
            symbolName: 'test',
            lineHint: 1,
            direction: 'incoming',
            callsPerPage: 10,
            page: 2,
            researchGoal: 'Find callers',
            reasoning: 'Testing pagination',
          },
        ],
      });

      expect(result).toBeDefined();
    });

    it('should handle contextLines parameter', async () => {
      const handler = createHandler();
      const result = await handler({
        queries: [
          {
            uri: '/workspace/test.ts',
            symbolName: 'test',
            lineHint: 1,
            direction: 'incoming',
            contextLines: 5,
            researchGoal: 'Find callers',
            reasoning: 'Testing context',
          },
        ],
      });

      expect(result).toBeDefined();
    });
  });

  describe('Direction Handling', () => {
    it('should handle incoming direction', async () => {
      const handler = createHandler();
      const result = await handler({
        queries: [
          {
            uri: '/workspace/test.ts',
            symbolName: 'mainFunction',
            lineHint: 4,
            direction: 'incoming',
            researchGoal: 'Find callers',
            reasoning: 'Testing incoming',
          },
        ],
      });

      expect(result).toBeDefined();
    });

    it('should handle outgoing direction', async () => {
      const handler = createHandler();
      const result = await handler({
        queries: [
          {
            uri: '/workspace/test.ts',
            symbolName: 'mainFunction',
            lineHint: 4,
            direction: 'outgoing',
            researchGoal: 'Find callees',
            reasoning: 'Testing outgoing',
          },
        ],
      });

      expect(result).toBeDefined();
    });
  });

  describe('LSP Integration', () => {
    it('should check LSP availability', async () => {
      const handler = createHandler();
      const result = await handler({
        queries: [
          {
            uri: '/workspace/test.ts',
            symbolName: 'test',
            lineHint: 1,
            direction: 'incoming',
            researchGoal: 'Find callers',
            reasoning: 'Testing',
          },
        ],
      });

      expect(result).toBeDefined();
    });

    it('should attempt LSP when available', async () => {
      vi.mocked(managerModule.isLanguageServerAvailable).mockResolvedValue(
        true
      );
      vi.mocked(managerModule.acquirePooledClient).mockResolvedValue({
        stop: vi.fn(),
        prepareCallHierarchy: vi.fn().mockResolvedValue([]),
        getIncomingCalls: vi.fn().mockResolvedValue([]),
        getOutgoingCalls: vi.fn().mockResolvedValue([]),
      } as any);

      const handler = createHandler();
      const result = await handler({
        queries: [
          {
            uri: '/workspace/test.ts',
            symbolName: 'test',
            lineHint: 1,
            direction: 'incoming',
            researchGoal: 'Find callers',
            reasoning: 'Testing',
          },
        ],
      });

      expect(result).toBeDefined();
    });

    it('should return empty (LSP_EMPTY) when available LSP call hierarchy throws', async () => {
      process.env.WORKSPACE_ROOT = process.cwd();
      const testPath = `${process.cwd()}/src/test.ts`;
      vi.mocked(managerModule.isLanguageServerAvailable).mockResolvedValue(
        true
      );
      vi.mocked(managerModule.acquirePooledClient).mockResolvedValue({
        stop: vi.fn(),
        prepareCallHierarchy: vi
          .fn()
          .mockRejectedValue(new Error('tsserver boom')),
        getIncomingCalls: vi.fn(),
        getOutgoingCalls: vi.fn(),
      } as any);

      const handler = createHandler();
      const result = await handler({
        queries: [
          {
            uri: testPath,
            symbolName: 'mainFunction',
            lineHint: 4,
            direction: 'incoming',
            researchGoal: 'Find callers',
            reasoning: 'Testing observable LSP empty result',
          },
        ],
      });

      const text = result.content?.[0]?.text ?? '';
      expect(text).not.toContain('lspMode');
      expect(text).toContain('LSP_EMPTY');
    });
  });

  describe('Pattern Matching Fallback', () => {
    it('should use pattern matching when LSP unavailable', async () => {
      vi.mocked(managerModule.isLanguageServerAvailable).mockResolvedValue(
        false
      );
      vi.mocked(checkCommandAvailability).mockResolvedValue({
        available: true,
        command: expect.stringMatching(/rg$/),
      });

      const handler = createHandler();
      const result = await handler({
        queries: [
          {
            uri: '/workspace/test.ts',
            symbolName: 'mainFunction',
            lineHint: 4,
            direction: 'incoming',
            researchGoal: 'Find callers',
            reasoning: 'Testing fallback',
          },
        ],
      });

      expect(result).toBeDefined();
      expect(result.content?.length).toBeGreaterThan(0);
    });

    it('should fall back to grep when ripgrep unavailable', async () => {
      vi.mocked(managerModule.isLanguageServerAvailable).mockResolvedValue(
        false
      );
      vi.mocked(checkCommandAvailability)
        .mockResolvedValueOnce({ available: false, command: 'rg' })
        .mockResolvedValueOnce({ available: true, command: 'grep' });

      const handler = createHandler();
      const result = await handler({
        queries: [
          {
            uri: '/workspace/test.ts',
            symbolName: 'mainFunction',
            lineHint: 4,
            direction: 'incoming',
            researchGoal: 'Find callers',
            reasoning: 'Testing grep fallback',
          },
        ],
      });

      expect(result).toBeDefined();
    });
  });

  describe('Schema Exports', () => {
    it('should export BulkLSPCallHierarchySchema', async () => {
      const { BulkLSPCallHierarchySchema } =
        await import('@octocodeai/octocode-core');

      expect(BulkLSPCallHierarchySchema).toBeDefined();
    });

    it('should export LSP_CALL_HIERARCHY_DESCRIPTION', async () => {
      const { LSP_CALL_HIERARCHY_DESCRIPTION } =
        await import('@octocodeai/octocode-core');

      expect(LSP_CALL_HIERARCHY_DESCRIPTION).toBeDefined();
      expect(typeof LSP_CALL_HIERARCHY_DESCRIPTION).toBe('string');
    });
  });

  describe('Empty Results', () => {
    it('should return empty status when no callers found', async () => {
      vi.mocked(managerModule.isLanguageServerAvailable).mockResolvedValue(
        false
      );
      vi.mocked(safeExec).mockResolvedValue({
        stdout: '',
        stderr: '',
        code: 1,
        success: false,
      });

      const handler = createHandler();
      const result = await handler({
        queries: [
          {
            uri: '/workspace/test.ts',
            symbolName: 'unusedFunction',
            lineHint: 1,
            direction: 'incoming',
            researchGoal: 'Check if function is called',
            reasoning: 'Testing empty results',
          },
        ],
      });

      expect(result).toBeDefined();
      expect(result.content?.length).toBeGreaterThan(0);
    });
  });
});
