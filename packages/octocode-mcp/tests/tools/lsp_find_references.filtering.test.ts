/**
 * Tests for file pattern filtering and lazy enhancement in lspFindReferences.
 *
 * Covers:
 * - matchesFilePatterns utility (picomatch-based glob matching)
 * - buildRipgrepGlobArgs (ripgrep --glob flag generation)
 * - buildGrepFilterArgs (grep --include/--exclude flag generation)
 * - Lazy enhancement in LSP path
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import type { LSPFindReferencesQuery } from '@octocodeai/octocode-core';

describe('File Pattern Filtering - Unit Tests', () => {
  describe('matchesFilePatterns', () => {
    let matchesFilePatterns: typeof import('../../src/tools/lsp_find_references/lspReferencesCore.js').matchesFilePatterns;

    beforeEach(async () => {
      const mod =
        await import('../../src/tools/lsp_find_references/lspReferencesCore.js');
      matchesFilePatterns = mod.matchesFilePatterns;
    });

    it('should return true when no patterns are specified', () => {
      expect(matchesFilePatterns('src/utils/helper.ts')).toBe(true);
      expect(
        matchesFilePatterns('src/utils/helper.ts', undefined, undefined)
      ).toBe(true);
      expect(matchesFilePatterns('src/utils/helper.ts', [], [])).toBe(true);
    });

    describe('includePattern only', () => {
      it('should match files matching include pattern', () => {
        expect(
          matchesFilePatterns('src/utils/helper.test.ts', ['**/*.test.ts'])
        ).toBe(true);
      });

      it('should reject files not matching include pattern', () => {
        expect(
          matchesFilePatterns('src/utils/helper.ts', ['**/*.test.ts'])
        ).toBe(false);
      });

      it('should match with multiple include patterns (OR logic)', () => {
        expect(
          matchesFilePatterns('src/utils/helper.spec.ts', [
            '**/*.test.ts',
            '**/*.spec.ts',
          ])
        ).toBe(true);
      });

      it('should match directory patterns', () => {
        expect(
          matchesFilePatterns('src/components/Button.tsx', ['**/src/**'])
        ).toBe(true);
        expect(
          matchesFilePatterns('lib/components/Button.tsx', ['**/src/**'])
        ).toBe(false);
      });
    });

    describe('excludePattern only', () => {
      it('should exclude files matching exclude pattern', () => {
        expect(
          matchesFilePatterns('node_modules/lodash/index.js', undefined, [
            '**/node_modules/**',
          ])
        ).toBe(false);
      });

      it('should include files not matching exclude pattern', () => {
        expect(
          matchesFilePatterns('src/utils/helper.ts', undefined, [
            '**/node_modules/**',
          ])
        ).toBe(true);
      });

      it('should exclude with multiple patterns', () => {
        expect(
          matchesFilePatterns('dist/bundle.js', undefined, [
            '**/node_modules/**',
            '**/dist/**',
          ])
        ).toBe(false);
      });
    });

    describe('includePattern + excludePattern combined', () => {
      it('should exclude takes precedence over include', () => {
        expect(
          matchesFilePatterns(
            'node_modules/pkg/index.test.ts',
            ['**/*.test.ts'],
            ['**/node_modules/**']
          )
        ).toBe(false);
      });

      it('should include file matching include and not matching exclude', () => {
        expect(
          matchesFilePatterns(
            'src/utils/helper.test.ts',
            ['**/*.test.ts'],
            ['**/node_modules/**']
          )
        ).toBe(true);
      });

      it('should reject file not matching include even if not excluded', () => {
        expect(
          matchesFilePatterns(
            'src/utils/helper.ts',
            ['**/*.test.ts'],
            ['**/node_modules/**']
          )
        ).toBe(false);
      });
    });

    describe('edge cases', () => {
      it('should handle simple filename patterns (non-recursive)', () => {
        // *.ts only matches files without directory prefix (picomatch default)
        expect(matchesFilePatterns('index.ts', ['*.ts'])).toBe(true);
        // For nested files, use **/*.ts
        expect(matchesFilePatterns('src/index.ts', ['**/*.ts'])).toBe(true);
        expect(matchesFilePatterns('src/index.ts', ['*.ts'])).toBe(false);
      });

      it('should handle deeply nested paths', () => {
        expect(
          matchesFilePatterns('packages/octocode-mcp/src/tools/lsp/index.ts', [
            '**/*.ts',
          ])
        ).toBe(true);
      });

      it('should handle extension-only exclude', () => {
        expect(
          matchesFilePatterns('src/data.json', undefined, ['**/*.json'])
        ).toBe(false);
      });
    });
  });

  describe('buildRipgrepGlobArgs', () => {
    let buildRipgrepGlobArgs: typeof import('../../src/tools/lsp_find_references/lspReferencesPatterns.js').buildRipgrepGlobArgs;
    let buildRipgrepSearchArgs: typeof import('../../src/tools/lsp_find_references/lspReferencesPatterns.js').buildRipgrepSearchArgs;

    beforeEach(async () => {
      const mod =
        await import('../../src/tools/lsp_find_references/lspReferencesPatterns.js');
      buildRipgrepGlobArgs = mod.buildRipgrepGlobArgs;
      buildRipgrepSearchArgs = mod.buildRipgrepSearchArgs;
    });

    it('should return empty array when no patterns', () => {
      expect(buildRipgrepGlobArgs()).toEqual([]);
      expect(buildRipgrepGlobArgs([], [])).toEqual([]);
    });

    it('should add --glob for include patterns', () => {
      const args = buildRipgrepGlobArgs(['**/*.test.ts', '**/src/**']);
      expect(args).toEqual(['--glob', '**/*.test.ts', '--glob', '**/src/**']);
    });

    it('should add --glob with ! prefix for exclude patterns', () => {
      const args = buildRipgrepGlobArgs(undefined, [
        '**/node_modules/**',
        '**/dist/**',
      ]);
      expect(args).toEqual([
        '--glob',
        '!**/node_modules/**',
        '--glob',
        '!**/dist/**',
      ]);
    });

    it('should combine include and exclude patterns', () => {
      const args = buildRipgrepGlobArgs(['**/*.ts'], ['**/node_modules/**']);
      expect(args).toEqual([
        '--glob',
        '**/*.ts',
        '--glob',
        '!**/node_modules/**',
      ]);
    });

    it('should add -- separator before symbol and workspace root', () => {
      const args = buildRipgrepSearchArgs('/workspace', '--pre=cat');
      const separatorIndex = args.indexOf('--');

      expect(separatorIndex).toBeGreaterThan(-1);
      expect(args[separatorIndex + 1]).toBe('--pre=cat');
      expect(args[separatorIndex + 2]).toBe('/workspace');
    });
  });
});

vi.mock('fs/promises', () => ({
  readFile: vi.fn(),
  stat: vi.fn(),
  access: vi.fn(),
}));

vi.mock('octocode-security-utils/pathValidator', () => ({
  pathValidator: {
    validate: vi.fn().mockReturnValue({ isValid: true }),
  },
}));

vi.mock('../../src/lsp/validation.js', async importOriginal => {
  const mod =
    await importOriginal<typeof import('../../src/lsp/validation.js')>();
  return {
    ...mod,
    safeReadFile: vi.fn(),
  };
});

vi.mock('../../src/lsp/resolver.js', () => ({
  SymbolResolver: vi.fn(),
  SymbolResolutionError: class extends Error {
    searchRadius: number;
    constructor(msg: string) {
      super(msg);
      this.searchRadius = 2;
    }
  },
}));

vi.mock('../../src/lsp/manager.js', () => ({
  LSP_UNAVAILABLE_HINT: 'LSP unavailable test',
  acquirePooledClient: vi.fn(),
  isLanguageServerAvailable: vi.fn(),
}));

import * as fs from 'fs/promises';
import * as managerModule from '../../src/lsp/manager.js';
import { safeReadFile } from '../../src/lsp/validation.js';
import { findReferencesWithLSP } from '../../src/tools/lsp_find_references/lspReferencesCore.js';

describe('LSP Find References - Filtering and Lazy Enhancement', () => {
  const mockClient = {
    findReferences: vi.fn(),
    stop: vi.fn(),
  };

  /** Helper to build a complete query with required defaults */
  function makeQuery(
    overrides: Partial<LSPFindReferencesQuery> &
      Pick<LSPFindReferencesQuery, 'uri' | 'symbolName' | 'lineHint'>
  ): LSPFindReferencesQuery {
    return {
      id: 'find_references_filtering_query',
      researchGoal: 'test',
      reasoning: 'test',
      orderHint: 0,
      includeDeclaration: true,
      contextLines: 0,
      page: 1,
      referencesPerPage: 20,
      ...overrides,
    };
  }

  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(managerModule.acquirePooledClient).mockResolvedValue(
      mockClient as any
    );
    const defaultContent =
      'line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10';
    vi.mocked(fs.readFile).mockResolvedValue(defaultContent);
    vi.mocked(safeReadFile).mockResolvedValue(defaultContent);
  });

  afterEach(() => {
    vi.resetAllMocks();
  });

  it('should filter results by includePattern', async () => {
    mockClient.findReferences.mockResolvedValue([
      {
        uri: '/workspace/src/utils/helper.ts',
        range: {
          start: { line: 2, character: 5 },
          end: { line: 2, character: 15 },
        },
        content: 'const testFunc = () => {};',
      },
      {
        uri: '/workspace/src/utils/helper.test.ts',
        range: {
          start: { line: 5, character: 10 },
          end: { line: 5, character: 20 },
        },
        content: 'expect(testFunc).toBeDefined();',
      },
      {
        uri: '/workspace/src/components/App.tsx',
        range: {
          start: { line: 1, character: 0 },
          end: { line: 1, character: 10 },
        },
        content: 'import { testFunc } from "../utils/helper";',
      },
    ]);

    const result = await findReferencesWithLSP(
      '/workspace/src/utils/helper.ts',
      '/workspace',
      { line: 2, character: 5 },
      makeQuery({
        uri: '/workspace/src/utils/helper.ts',
        symbolName: 'testFunc',
        lineHint: 3,
        includePattern: ['**/*.test.ts'],
      })
    );

    expect(result).not.toBeNull();
    expect(result!.status).toBeUndefined();
    expect(result!.locations).toHaveLength(1);
    expect(result!.locations![0]!.uri).toContain('helper.test.ts');
  });

  it('should filter results by excludePattern', async () => {
    mockClient.findReferences.mockResolvedValue([
      {
        uri: '/workspace/src/utils/helper.ts',
        range: {
          start: { line: 2, character: 5 },
          end: { line: 2, character: 15 },
        },
        content: 'const testFunc = () => {};',
      },
      {
        uri: '/workspace/node_modules/pkg/index.js',
        range: {
          start: { line: 1, character: 0 },
          end: { line: 1, character: 10 },
        },
        content: 'module.exports = { testFunc };',
      },
    ]);

    const result = await findReferencesWithLSP(
      '/workspace/src/utils/helper.ts',
      '/workspace',
      { line: 2, character: 5 },
      makeQuery({
        uri: '/workspace/src/utils/helper.ts',
        symbolName: 'testFunc',
        lineHint: 3,
        excludePattern: ['**/node_modules/**'],
      })
    );

    expect(result).not.toBeNull();
    expect(result!.status).toBeUndefined();
    expect(result!.locations).toHaveLength(1);
    expect(result!.locations![0]!.uri).not.toContain('node_modules');
  });

  it('should return empty when all references are filtered out', async () => {
    mockClient.findReferences.mockResolvedValue([
      {
        uri: '/workspace/node_modules/pkg/index.js',
        range: {
          start: { line: 1, character: 0 },
          end: { line: 1, character: 10 },
        },
        content: 'module.exports = { testFunc };',
      },
    ]);

    const result = await findReferencesWithLSP(
      '/workspace/src/utils/helper.ts',
      '/workspace',
      { line: 2, character: 5 },
      makeQuery({
        uri: '/workspace/src/utils/helper.ts',
        symbolName: 'testFunc',
        lineHint: 3,
        excludePattern: ['**/node_modules/**'],
      })
    );

    expect(result).not.toBeNull();
    expect(result!.status).toBe('empty');
    expect(result!.hints).toBeDefined();
    expect(result!.hints!.some(h => h.includes('none matched'))).toBe(true);
  });

  it('should include filter hint when results are filtered', async () => {
    mockClient.findReferences.mockResolvedValue([
      {
        uri: '/workspace/src/a.ts',
        range: {
          start: { line: 0, character: 0 },
          end: { line: 0, character: 5 },
        },
        content: 'import { x } from "./b";',
      },
      {
        uri: '/workspace/src/b.ts',
        range: {
          start: { line: 0, character: 0 },
          end: { line: 0, character: 5 },
        },
        content: 'export const x = 1;',
      },
      {
        uri: '/workspace/node_modules/pkg/c.js',
        range: {
          start: { line: 0, character: 0 },
          end: { line: 0, character: 5 },
        },
        content: 'const x = require("./d");',
      },
    ]);

    const result = await findReferencesWithLSP(
      '/workspace/src/a.ts',
      '/workspace',
      { line: 0, character: 0 },
      makeQuery({
        uri: '/workspace/src/a.ts',
        symbolName: 'x',
        lineHint: 1,
        excludePattern: ['**/node_modules/**'],
      })
    );

    expect(result).not.toBeNull();
    expect(result!.locations).toHaveLength(2);
    expect(result!.hints!.some(h => h.includes('Filtered: 2 of 3'))).toBe(true);
  });

  it('should apply lazy enhancement only to paginated items', async () => {
    const refs = Array.from({ length: 5 }, (_, i) => ({
      uri: `/workspace/src/file${i}.ts`,
      range: {
        start: { line: i, character: 0 },
        end: { line: i, character: 5 },
      },
      content: `line ${i}`,
    }));
    mockClient.findReferences.mockResolvedValue(refs);

    let readCount = 0;
    vi.mocked(safeReadFile).mockImplementation(async () => {
      readCount++;
      return 'line1\nline2\nline3\nline4\nline5';
    });

    const result = await findReferencesWithLSP(
      '/workspace/src/file0.ts',
      '/workspace',
      { line: 0, character: 0 },
      makeQuery({
        uri: '/workspace/src/file0.ts',
        symbolName: 'test',
        lineHint: 1,
        contextLines: 2,
        referencesPerPage: 2,
      })
    );

    expect(result).not.toBeNull();
    expect(result!.locations).toHaveLength(2);
    // readFile called only for the 2 paginated items, not all 5
    expect(readCount).toBe(2);
  });

  it('should return empty status with pagination when page exceeds total pages', async () => {
    mockClient.findReferences.mockResolvedValue([
      {
        uri: '/workspace/src/a.ts',
        range: {
          start: { line: 0, character: 0 },
          end: { line: 0, character: 5 },
        },
        content: 'const x = 1;',
      },
      {
        uri: '/workspace/src/b.ts',
        range: {
          start: { line: 1, character: 0 },
          end: { line: 1, character: 5 },
        },
        content: 'const x = 2;',
      },
    ]);

    const result = await findReferencesWithLSP(
      '/workspace/src/a.ts',
      '/workspace',
      { line: 0, character: 0 },
      makeQuery({
        uri: '/workspace/src/a.ts',
        symbolName: 'x',
        lineHint: 1,
        referencesPerPage: 1,
        page: 5,
      })
    );

    expect(result).not.toBeNull();
    expect(result!.status).toBe('empty');
    expect(result!.pagination).toEqual({
      currentPage: 5,
      totalPages: 2,
      totalResults: 2,
      hasMore: false,
      resultsPerPage: 1,
    });
    expect(
      result!.hints!.some(h => h.includes('outside available range'))
    ).toBe(true);
  });

  it('should skip enhancement when contextLines is 0', async () => {
    mockClient.findReferences.mockResolvedValue([
      {
        uri: '/workspace/src/file.ts',
        range: {
          start: { line: 0, character: 0 },
          end: { line: 0, character: 5 },
        },
        content: 'const x = 1;',
      },
    ]);

    const result = await findReferencesWithLSP(
      '/workspace/src/file.ts',
      '/workspace',
      { line: 0, character: 0 },
      makeQuery({
        uri: '/workspace/src/file.ts',
        symbolName: 'x',
        lineHint: 1,
        contextLines: 0,
      })
    );

    expect(result).not.toBeNull();
    expect(result!.locations).toHaveLength(1);
    expect(fs.readFile).not.toHaveBeenCalled();
  });

  it('should return null when client creation fails', async () => {
    vi.mocked(managerModule.acquirePooledClient).mockResolvedValue(null);

    const result = await findReferencesWithLSP(
      '/workspace/src/file.ts',
      '/workspace',
      { line: 0, character: 0 },
      makeQuery({
        uri: '/workspace/src/file.ts',
        symbolName: 'x',
        lineHint: 1,
      })
    );

    expect(result).toBeNull();
  });

  it('must NOT stop the pooled client when the LSP call throws', async () => {
    // Regression: the pre-pool implementation stopped the client in a finally
    // block. With the shared pool, the caller MUST NOT touch lifecycle —
    // idle eviction handles teardown. Stopping here would kill warm
    // tsserver state for every other caller of the same project.
    mockClient.findReferences.mockRejectedValue(new Error('LSP error'));

    try {
      await findReferencesWithLSP(
        '/workspace/src/file.ts',
        '/workspace',
        { line: 0, character: 0 },
        makeQuery({
          uri: '/workspace/src/file.ts',
          symbolName: 'x',
          lineHint: 1,
        })
      );
    } catch {
      // findReferencesWithLSP currently rethrows when the LSP call errors;
      // accept that — the contract under test is the no-stop guarantee.
    }

    expect(mockClient.stop).not.toHaveBeenCalled();
  });

  it('should combine includePattern and excludePattern', async () => {
    mockClient.findReferences.mockResolvedValue([
      {
        uri: '/workspace/src/utils/helper.test.ts',
        range: {
          start: { line: 0, character: 0 },
          end: { line: 0, character: 5 },
        },
        content: 'test content',
      },
      {
        uri: '/workspace/src/utils/helper.ts',
        range: {
          start: { line: 0, character: 0 },
          end: { line: 0, character: 5 },
        },
        content: 'source content',
      },
      {
        uri: '/workspace/node_modules/pkg/helper.test.ts',
        range: {
          start: { line: 0, character: 0 },
          end: { line: 0, character: 5 },
        },
        content: 'node_modules test',
      },
    ]);

    const result = await findReferencesWithLSP(
      '/workspace/src/utils/helper.ts',
      '/workspace',
      { line: 0, character: 0 },
      makeQuery({
        uri: '/workspace/src/utils/helper.ts',
        symbolName: 'helper',
        lineHint: 1,
        includePattern: ['**/*.test.ts'],
        excludePattern: ['**/node_modules/**'],
      })
    );

    expect(result).not.toBeNull();
    expect(result!.locations).toHaveLength(1);
    expect(result!.locations![0]!.uri).toBe(
      '/workspace/src/utils/helper.test.ts'
    );
  });

  it('should return all results when no patterns specified', async () => {
    mockClient.findReferences.mockResolvedValue([
      {
        uri: '/workspace/src/a.ts',
        range: {
          start: { line: 0, character: 0 },
          end: { line: 0, character: 5 },
        },
        content: 'content a',
      },
      {
        uri: '/workspace/src/b.ts',
        range: {
          start: { line: 0, character: 0 },
          end: { line: 0, character: 5 },
        },
        content: 'content b',
      },
    ]);

    const result = await findReferencesWithLSP(
      '/workspace/src/a.ts',
      '/workspace',
      { line: 0, character: 0 },
      makeQuery({
        uri: '/workspace/src/a.ts',
        symbolName: 'x',
        lineHint: 1,
      })
    );

    expect(result).not.toBeNull();
    expect(result!.locations).toHaveLength(2);
  });
});
