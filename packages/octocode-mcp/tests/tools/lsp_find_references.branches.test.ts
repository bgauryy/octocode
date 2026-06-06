import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import type { LSPFindReferencesQuery } from '@octocodeai/octocode-core';

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

vi.mock('../../src/tools/lsp_find_references/lspReferencesCore.js', () => ({
  findReferencesWithLSP: vi.fn().mockResolvedValue(null),
}));

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

import * as fs from 'fs/promises';
import * as resolverModule from '../../src/lsp/resolver.js';
import * as managerModule from '../../src/lsp/manager.js';
import * as coreModule from '../../src/tools/lsp_find_references/lspReferencesCore.js';
import {
  validateToolPath,
  createErrorResult,
} from '../../src/utils/file/toolHelpers.js';
import type { FindReferencesResult } from '../../src/lsp/types.js';
import {
  applyFindReferencesVerbosity,
  findReferences,
} from '../../src/tools/lsp_find_references/lsp_find_references.js';

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

    vi.mocked(resolverModule.SymbolResolver).mockImplementation(function () {
      return {
        resolvePositionFromContent: vi.fn().mockReturnValue({
          position: { line: 0, character: 9 },
          foundAtLine: 1,
        }),
      };
    });

    vi.mocked(coreModule.findReferencesWithLSP).mockResolvedValue(null);
  });

  afterEach(() => {
    delete process.env.WORKSPACE_ROOT;
    vi.resetAllMocks();
  });

  describe('groupByFile structured output', () => {
    it('returns a ranked byFile rollup instead of burying the map in hints', () => {
      const result: FindReferencesResult = {
        locations: [
          {
            uri: '/workspace/src/b.ts',
            range: {
              start: { line: 4, character: 2 },
              end: { line: 4, character: 14 },
            },
            content: 'use testFunction',
          },
          {
            uri: '/workspace/src/a.ts',
            range: {
              start: { line: 1, character: 0 },
              end: { line: 1, character: 12 },
            },
            content: 'testFunction();',
            isDefinition: true,
          },
          {
            uri: '/workspace/src/b.ts',
            range: {
              start: { line: 8, character: 4 },
              end: { line: 8, character: 16 },
            },
            content: 'testFunction();',
          },
        ],
      };

      const grouped = applyFindReferencesVerbosity(result, {
        ...baseQuery,
        groupByFile: true,
      });

      expect(grouped.locations).toEqual([]);
      expect(grouped.totalReferences).toBe(3);
      expect(grouped.totalFiles).toBe(2);
      expect(grouped.byFile).toEqual([
        {
          uri: '/workspace/src/b.ts',
          count: 2,
          firstLine: 5,
          firstCharacter: 2,
          lines: [5, 9],
        },
        {
          uri: '/workspace/src/a.ts',
          count: 1,
          firstLine: 2,
          firstCharacter: 0,
          lines: [2],
          hasDefinition: true,
        },
      ]);
      expect(grouped.hints?.some(hint => hint.startsWith('byFile:'))).toBe(
        false
      );
    });

    it('byFile entries include all reference line numbers in lines[]', () => {
      const result: FindReferencesResult = {
        locations: [
          {
            uri: '/workspace/src/b.ts',
            range: {
              start: { line: 4, character: 2 },
              end: { line: 4, character: 14 },
            },
            content: 'use testFunction',
          },
          {
            uri: '/workspace/src/b.ts',
            range: {
              start: { line: 8, character: 4 },
              end: { line: 8, character: 16 },
            },
            content: 'testFunction();',
          },
          {
            uri: '/workspace/src/b.ts',
            range: {
              start: { line: 9, character: 0 },
              end: { line: 9, character: 12 },
            },
            content: 'testFunction();',
          },
        ],
      };

      const grouped = applyFindReferencesVerbosity(result, {
        ...baseQuery,
        groupByFile: true,
      });

      const bEntry = grouped.byFile?.[0];
      expect(bEntry?.lines).toBeDefined();
      expect(bEntry?.lines).toHaveLength(3);
      expect(bEntry?.lines).toContain(5);
      expect(bEntry?.lines).toContain(9);
      expect(bEntry?.lines).toContain(10);
    });

    it('rolls up ALL references in groupByFile, not just the current page', async () => {
      vi.mocked(managerModule.isLanguageServerAvailable).mockResolvedValue(
        true
      );
      const make = (uri: string, line: number) => ({
        uri,
        range: {
          start: { line, character: 2 },
          end: { line, character: 14 },
        },
        content: 'testFunction();',
      });
      const allLocations = [
        ...Array.from({ length: 12 }, (_, i) => make('/workspace/src/a.ts', i)),
        ...Array.from({ length: 8 }, (_, i) => make('/workspace/src/b.ts', i)),
        ...Array.from({ length: 5 }, (_, i) => make('/workspace/src/c.ts', i)),
      ];

      vi.mocked(coreModule.findReferencesWithLSP).mockResolvedValue({
        locations: allLocations,
        totalReferences: allLocations.length,
        researchGoal: 'test',
        reasoning: 'test',
      } as any);

      const grouped = await findReferences({
        ...baseQuery,
        groupByFile: true,
        page: 1,
        referencesPerPage: 10,
      });

      expect(grouped.totalReferences).toBe(25);
      expect(grouped.totalFiles).toBe(3);
      const sum = (grouped.byFile ?? []).reduce((acc, f) => acc + f.count, 0);
      expect(sum).toBe(25);
      expect(grouped.byFile?.map(f => [f.uri, f.count])).toEqual([
        ['/workspace/src/a.ts', 12],
        ['/workspace/src/b.ts', 8],
        ['/workspace/src/c.ts', 5],
      ]);
    });
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

      const result = await findReferences(baseQuery);
      expect(result.status).toBe('error');
    });

    it('should return an empty LSP-not-installed result when no language server is available', async () => {
      vi.mocked(managerModule.isLanguageServerAvailable).mockResolvedValue(
        false
      );

      const result = await findReferences(baseQuery);

      expect(result.status).toBe('empty');
      expect(result.errorCode).toBe('LSP_NOT_INSTALLED');
      expect(result.locations).toBeUndefined();
      expect(coreModule.findReferencesWithLSP).not.toHaveBeenCalled();
      expect(result.hints?.some(h => h.includes('localSearchCode'))).toBe(true);
    });

    it('should return an empty LSP-empty result when LSP is available but returns nothing', async () => {
      vi.mocked(managerModule.isLanguageServerAvailable).mockResolvedValue(
        true
      );
      vi.mocked(coreModule.findReferencesWithLSP).mockResolvedValue(null);

      const result = await findReferences(baseQuery);

      expect(result.status).toBe('empty');
      expect(result.errorCode).toBe('LSP_EMPTY');
      expect(result.locations).toBeUndefined();
      expect(coreModule.findReferencesWithLSP).toHaveBeenCalled();
    });

    it('should return an empty LSP-empty result when LSP throws', async () => {
      vi.mocked(managerModule.isLanguageServerAvailable).mockResolvedValue(
        true
      );
      vi.mocked(coreModule.findReferencesWithLSP).mockRejectedValue(
        new Error('LSP crashed')
      );

      const result = await findReferences(baseQuery);

      expect(result.status).toBe('empty');
      expect(result.errorCode).toBe('LSP_EMPTY');
      expect(coreModule.findReferencesWithLSP).toHaveBeenCalled();
    });

    it('should return LSP semantic results when both available and populated', async () => {
      vi.mocked(managerModule.isLanguageServerAvailable).mockResolvedValue(
        true
      );
      const lspResult = {
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

      expect(result.status).toBeUndefined();
      expect(coreModule.findReferencesWithLSP).toHaveBeenCalled();
    });
  });

  describe('global pagination semantics', () => {
    it('should page over the full LSP dataset', async () => {
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

      vi.mocked(coreModule.findReferencesWithLSP).mockResolvedValue({
        locations: [makeLocation('src/lspB.ts', 2)],
        pagination: {
          currentPage: 2,
          totalPages: 4,
          totalResults: 4,
          hasMore: true,
          resultsPerPage: 1,
        },
        hints: [],
      } as any);

      const result = await findReferences({
        ...baseQuery,
        page: 2,
        referencesPerPage: 1,
      });

      expect(result.status).toBeUndefined();
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
