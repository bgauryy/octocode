/**
 * Branch coverage tests for LSP Find References tool
 * Targets uncovered branches in lsp_find_references.ts and lspReferencesCore.ts
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import type { LSPFindReferencesQuery } from '@octocodeai/octocode-core';

// Mock fs/promises
vi.mock('fs/promises', () => ({
  stat: vi.fn(),
  readFile: vi.fn(),
}));

vi.mock('../../src/lsp/resolver.js', () => {
  class MockSymbolResolutionError extends Error {
    searchRadius: number;
    symbolName: string;
    lineHint: number;
    constructor(message: string, searchRadius: number = 5) {
      super(message);
      this.name = 'SymbolResolutionError';
      this.searchRadius = searchRadius;
      this.symbolName = '';
      this.lineHint = 0;
    }
  }

  return {
    SymbolResolver: vi.fn().mockImplementation(() => ({
      resolvePositionFromContent: vi.fn().mockReturnValue({
        position: { line: 0, character: 9 },
        foundAtLine: 1,
      }),
    })),
    SymbolResolutionError: MockSymbolResolutionError,
  };
});

vi.mock('../../src/lsp/manager.js', () => ({
  LSP_UNAVAILABLE_HINT: 'LSP unavailable test',
  isLanguageServerAvailable: vi.fn().mockResolvedValue(false),
  acquirePooledClient: vi.fn().mockResolvedValue(null),
}));

// Mock pattern matching module
vi.mock('../../src/tools/lsp_find_references/lspReferencesPatterns.js', () => ({
  findReferencesWithPatternMatching: vi.fn().mockResolvedValue({
    status: 'hasResults',
    locations: [],
    totalReferences: 0,
  }),
  findWorkspaceRoot: vi.fn(async () => '/workspace'),
  isLikelyDefinition: vi.fn(),
}));

// Mock lspReferencesCore
vi.mock('../../src/tools/lsp_find_references/lspReferencesCore.js', () => ({
  findReferencesWithLSP: vi.fn().mockResolvedValue(null),
}));

// Mock toolHelpers
vi.mock('../../src/utils/file/toolHelpers.js', () => ({
  validateToolPath: vi.fn().mockReturnValue({
    isValid: true,
    sanitizedPath: '/workspace/src/file.ts',
  }),
  createErrorResult: vi.fn((error: unknown, _query: unknown) => ({
    status: 'error',
    error: error instanceof Error ? error.message : String(error),
    errorType: 'tool_error',
  })),
}));

vi.mock('../../src/errors/errorFactories.js', () => ({
  ToolErrors: {
    fileAccessFailed: vi.fn(
      (path: string) => new Error(`Cannot access file: ${path}`)
    ),
    fileReadFailed: vi.fn(
      (path: string) => new Error(`Failed to read file: ${path}`)
    ),
  },
}));

// Import after mocks
import * as fs from 'fs/promises';
import * as resolverModule from '../../src/lsp/resolver.js';
import * as managerModule from '../../src/lsp/manager.js';
import * as patternModule from '../../src/tools/lsp_find_references/lspReferencesPatterns.js';
import * as coreModule from '../../src/tools/lsp_find_references/lspReferencesCore.js';
import {
  validateToolPath,
  createErrorResult,
} from '../../src/utils/file/toolHelpers.js';
import { findReferences } from '../../src/tools/lsp_find_references/lsp_find_references.js';

describe('LSP Find References - Branch Coverage Tests', () => {
  const baseQuery: LSPFindReferencesQuery = {
    id: 'find_references_branch_query',
    uri: '/workspace/src/file.ts',
    symbolName: 'testFunction',
    lineHint: 5,
    orderHint: 0,
    contextLines: 2,
    page: 1,
    includeDeclaration: true,
    referencesPerPage: 20,
    researchGoal: 'test',
    reasoning: 'test',
  };

  beforeEach(() => {
    vi.clearAllMocks();
    process.env.WORKSPACE_ROOT = '/workspace';

    // Re-setup all mocks after clearAllMocks
    vi.mocked(validateToolPath).mockReturnValue({
      isValid: true,
      sanitizedPath: '/workspace/src/file.ts',
    } as ReturnType<typeof validateToolPath>);
    vi.mocked(createErrorResult).mockImplementation(
      (error: unknown, _query: any, _options?: any) =>
        ({
          status: 'error',
          error: error instanceof Error ? error.message : String(error),
          errorType: 'tool_error',
        }) as any
    );
    vi.mocked(fs.stat).mockResolvedValue({} as any);
    vi.mocked(fs.readFile).mockResolvedValue(
      'function testFunction() {}\nexport { testFunction };'
    );
    vi.mocked(managerModule.isLanguageServerAvailable).mockResolvedValue(false);
    vi.mocked(managerModule.acquirePooledClient).mockResolvedValue(null);

    // Must use regular function (not arrow) because it's called with `new`
    vi.mocked(resolverModule.SymbolResolver).mockImplementation(function () {
      return {
        resolvePositionFromContent: vi.fn().mockReturnValue({
          position: { line: 0, character: 9 },
          foundAtLine: 1,
        }),
      };
    });

    vi.mocked(
      patternModule.findReferencesWithPatternMatching
    ).mockResolvedValue({
      status: 'hasResults',
      locations: [],
      totalReferences: 0,
      researchGoal: 'test',
      reasoning: 'test',
    } as any);

    vi.mocked(patternModule.findWorkspaceRoot).mockResolvedValue('/workspace');
    vi.mocked(coreModule.findReferencesWithLSP).mockResolvedValue(null);
  });

  afterEach(() => {
    delete process.env.WORKSPACE_ROOT;
    vi.resetAllMocks();
  });

  describe('lsp_find_references.ts - Error Paths', () => {
    it('should handle file stat failure (lines 84-88)', async () => {
      vi.mocked(fs.stat).mockRejectedValue(new Error('ENOENT'));

      const result = await findReferences(baseQuery);

      expect(result.status).toBe('error');
      expect(fs.stat).toHaveBeenCalled();
    });

    it('should handle file readFile failure (lines 99-103)', async () => {
      vi.mocked(fs.readFile).mockRejectedValue(new Error('EPERM'));

      const result = await findReferences(baseQuery);

      expect(result.status).toBe('error');
      expect(fs.readFile).toHaveBeenCalled();
    });

    it('should handle SymbolResolutionError (line 120)', async () => {
      // Override SymbolResolver to throw SymbolResolutionError
      const { SymbolResolutionError } = resolverModule;
      vi.mocked(resolverModule.SymbolResolver).mockImplementation(function () {
        return {
          resolvePositionFromContent: vi.fn().mockImplementation(() => {
            throw new SymbolResolutionError(
              'testFunction',
              5,
              'Symbol not found near line 5',
              2
            );
          }),
        };
      });

      const result = await findReferences(baseQuery);

      expect(result.status).toBe('empty');
      expect(result.errorType).toBe('symbol_not_found');
      expect(result.hints).toBeDefined();
      expect(result.hints!.length).toBeGreaterThan(0);
    });

    it('should rethrow non-SymbolResolutionError (line 135) - caught by outer catch', async () => {
      vi.mocked(resolverModule.SymbolResolver).mockImplementation(function () {
        return {
          resolvePositionFromContent: vi.fn().mockImplementation(() => {
            throw new TypeError('unexpected error');
          }),
        };
      });

      // The outer catch wraps it via createErrorResult
      const result = await findReferences(baseQuery);
      expect(result.status).toBe('error');
    });

    it('should fallback to pattern matching when LSP returns null (line 151)', async () => {
      vi.mocked(managerModule.isLanguageServerAvailable).mockResolvedValue(
        true
      );
      vi.mocked(coreModule.findReferencesWithLSP).mockResolvedValue(null);

      vi.mocked(
        patternModule.findReferencesWithPatternMatching
      ).mockResolvedValue({
        status: 'hasResults',
        locations: [{ uri: '/workspace/src/file.ts', range: {} }],
        totalReferences: 1,
        researchGoal: 'test',
        reasoning: 'test',
      } as any);

      const result = await findReferences(baseQuery);

      expect(result.status).toBe('hasResults');
      expect(
        patternModule.findReferencesWithPatternMatching
      ).toHaveBeenCalled();
    });

    it('should fallback to pattern matching when LSP throws (line 152)', async () => {
      vi.mocked(managerModule.isLanguageServerAvailable).mockResolvedValue(
        true
      );
      vi.mocked(coreModule.findReferencesWithLSP).mockRejectedValue(
        new Error('LSP crashed')
      );

      const result = await findReferences(baseQuery);

      expect(result.status).toBe('hasResults');
      expect(
        patternModule.findReferencesWithPatternMatching
      ).toHaveBeenCalled();
    });

    it('should merge LSP and pattern results when both return data (line 151)', async () => {
      vi.mocked(managerModule.isLanguageServerAvailable).mockResolvedValue(
        true
      );
      const lspResult = {
        status: 'hasResults' as const,
        locations: [{ uri: '/test.ts', range: { start: { line: 0 } } }],
        totalReferences: 1,
        researchGoal: 'test',
        reasoning: 'test',
        hints: [],
      };
      vi.mocked(coreModule.findReferencesWithLSP).mockResolvedValue(
        lspResult as any
      );

      const result = await findReferences(baseQuery);

      expect(result.status).toBe('hasResults');
      // Pattern matching is always called for hybrid merge
      expect(
        patternModule.findReferencesWithPatternMatching
      ).toHaveBeenCalled();
    });

    it('should use process.cwd() when WORKSPACE_ROOT env not set (line 137)', async () => {
      delete process.env.WORKSPACE_ROOT;
      const cwdSpy = vi.spyOn(process, 'cwd').mockReturnValue('/mock/cwd');

      await findReferences(baseQuery);

      expect(cwdSpy).toHaveBeenCalled();
      // findWorkspaceRoot is no longer called - process.cwd() is used instead
      expect(patternModule.findWorkspaceRoot).not.toHaveBeenCalled();
      cwdSpy.mockRestore();
    });
  });

  describe('global merge semantics', () => {
    it('should build merged pagination from full branch datasets before final paging', async () => {
      vi.mocked(managerModule.isLanguageServerAvailable).mockResolvedValue(
        true
      );

      const makeLocation = (uri: string, line: number) => ({
        uri,
        range: {
          start: { line, character: 0 },
          end: { line, character: 4 },
        },
        content: `${uri}:${line}`,
        isDefinition: false,
      });

      vi.mocked(coreModule.findReferencesWithLSP).mockImplementation(
        async (_filePath, _workspaceRoot, _position, q) => {
          const query = q as LSPFindReferencesQuery;
          const isGlobalRequest =
            query.page === 1 &&
            typeof query.referencesPerPage === 'number' &&
            query.referencesPerPage > 1000;
          return {
            status: 'hasResults',
            locations: isGlobalRequest
              ? [makeLocation('src/lspA.ts', 1), makeLocation('src/lspB.ts', 2)]
              : [makeLocation('src/lspA.ts', 1)],
            hints: [],
          } as any;
        }
      );

      vi.mocked(
        patternModule.findReferencesWithPatternMatching
      ).mockImplementation(async (_absolutePath, _workspaceRoot, q) => {
        const query = q as LSPFindReferencesQuery;
        const isGlobalRequest =
          query.page === 1 &&
          typeof query.referencesPerPage === 'number' &&
          query.referencesPerPage > 1000;
        return {
          status: 'hasResults',
          locations: isGlobalRequest
            ? [
                makeLocation('src/patternA.ts', 3),
                makeLocation('src/patternB.ts', 4),
              ]
            : [makeLocation('src/patternA.ts', 3)],
          hints: [],
        } as any;
      });

      const result = await findReferences({
        ...baseQuery,
        page: 2,
        referencesPerPage: 1,
      });

      expect(result.status).toBe('hasResults');
      expect(result.pagination).toEqual(
        expect.objectContaining({
          currentPage: 2,
          totalPages: 4,
          totalResults: 4,
          hasMore: true,
          resultsPerPage: 1,
        })
      );
      expect(result.locations).toHaveLength(1);
    });
  });
});
