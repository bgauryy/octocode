/**
 * Branch coverage tests for lsp_goto_definition/execution.ts
 * Targets: file read catch block (line 351-353), output limit branch (lines 512-529)
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import * as fs from 'fs/promises';

vi.mock('fs/promises', () => ({
  readFile: vi.fn(),
  stat: vi.fn(),
}));

vi.mock('../../src/utils/file/toolHelpers.js', () => ({
  validateToolPath: vi.fn().mockReturnValue({
    isValid: true,
    sanitizedPath: '/workspace/src/source.ts',
  }),
  createErrorResult: vi.fn((err: unknown) => ({
    status: 'error',
    error: err instanceof Error ? err.message : String(err),
    errorCode: 'toolExecutionFailed',
  })),
}));

vi.mock('../../src/lsp/validation.js', async importOriginal => {
  const mod =
    await importOriginal<typeof import('../../src/lsp/validation.js')>();
  return {
    ...mod,
    safeReadFile: vi.fn(),
  };
});

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
    SymbolResolver: vi.fn().mockImplementation(function () {
      return {
        resolvePositionFromContent: vi.fn().mockReturnValue({
          position: { line: 3, character: 16 },
          foundAtLine: 4,
        }),
        extractContext: vi.fn().mockReturnValue({
          content: 'test content',
          startLine: 1,
          endLine: 10,
        }),
      };
    }),
    SymbolResolutionError: MockSymbolResolutionError,
  };
});

vi.mock('../../src/lsp/manager.js', () => ({
  LSP_UNAVAILABLE_HINT: 'LSP unavailable test',
  acquirePooledClient: vi.fn(),
  isLanguageServerAvailable: vi.fn().mockResolvedValue(true),
}));

import * as resolverModule from '../../src/lsp/resolver.js';
import * as managerModule from '../../src/lsp/manager.js';
import { safeReadFile } from '../../src/lsp/validation.js';
import * as toolHelpers from '../../src/utils/file/toolHelpers.js';

describe('LSP Goto Definition - Branch Coverage', () => {
  const mockClient = {
    gotoDefinition: vi.fn(),
    stop: vi.fn(),
  };

  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(toolHelpers.validateToolPath).mockReturnValue({
      isValid: true,
      sanitizedPath: '/workspace/src/source.ts',
    });
    vi.mocked(resolverModule.SymbolResolver).mockImplementation(function () {
      return {
        resolvePositionFromContent: vi.fn().mockReturnValue({
          position: { line: 3, character: 16 },
          foundAtLine: 4,
        }),
        extractContext: vi.fn().mockReturnValue({
          content: 'test content',
          startLine: 1,
          endLine: 10,
        }),
      };
    });
    vi.mocked(managerModule.acquirePooledClient).mockResolvedValue(
      mockClient as any
    );
    vi.mocked(managerModule.isLanguageServerAvailable).mockResolvedValue(true);
    vi.mocked(fs.stat).mockResolvedValue({ isFile: () => true } as any);
    const sampleContent = 'line1\nline2\nconst myFunc = () => {};\nline4';
    vi.mocked(fs.readFile).mockResolvedValue(sampleContent);
  });

  describe('file read catch block (line 351-353)', () => {
    it('should keep original location when safeReadFile fails during enhancement', async () => {
      mockClient.gotoDefinition.mockResolvedValue([
        {
          uri: '/workspace/src/target.ts',
          range: {
            start: { line: 2, character: 6 },
            end: { line: 2, character: 12 },
          },
        },
      ]);

      // safeReadFile fails → triggers catch block at line 351
      vi.mocked(safeReadFile).mockRejectedValue(
        new Error('File read permission denied')
      );

      const { registerLSPGotoDefinitionTool } =
        await import('../../src/tools/lsp_goto_definition/lsp_goto_definition.js');

      const mockServer = {
        registerTool: vi.fn(
          (_name: string, _config: any, handler: any) => handler
        ),
      };
      registerLSPGotoDefinitionTool(mockServer as any);
      const handler = mockServer.registerTool.mock.results[0]!.value;

      const result = await handler({
        queries: [
          {
            uri: '/workspace/src/source.ts',
            symbolName: 'myFunc',
            lineHint: 3,
            researchGoal: 'Test catch branch',
            reasoning: 'Testing file read failure',
          },
        ],
      });

      expect(result).toBeDefined();
      const text = result.content?.[0]?.text ?? '';
      // Should have results (even if enhancement failed, location is kept)
      expect(text).toContain('hasResults');
    });
  });

  describe('output limit pagination (lines 512-529)', () => {
    it('should apply output pagination when charLength is very small', async () => {
      vi.mocked(safeReadFile).mockResolvedValue(
        'line1\nline2\nconst myFunc = () => {};\nline4'
      );

      mockClient.gotoDefinition.mockResolvedValue([
        {
          uri: '/workspace/src/target.ts',
          range: {
            start: { line: 2, character: 6 },
            end: { line: 2, character: 12 },
          },
        },
      ]);

      const { registerLSPGotoDefinitionTool } =
        await import('../../src/tools/lsp_goto_definition/lsp_goto_definition.js');

      const mockServer = {
        registerTool: vi.fn(
          (_name: string, _config: any, handler: any) => handler
        ),
      };
      registerLSPGotoDefinitionTool(mockServer as any);
      const handler = mockServer.registerTool.mock.results[0]!.value;

      const result = await handler({
        queries: [
          {
            uri: '/workspace/src/source.ts',
            symbolName: 'myFunc',
            lineHint: 3,
            charLength: 10,
            researchGoal: 'Test output limit',
            reasoning: 'Testing pagination',
          },
        ],
      });

      expect(result).toBeDefined();
      const text = result.content?.[0]?.text ?? '';
      expect(text).toContain('hasResults');
      expect(text).toMatch(/charOffset|charLength|outputPagination|hasMore/);
    });
  });
});
