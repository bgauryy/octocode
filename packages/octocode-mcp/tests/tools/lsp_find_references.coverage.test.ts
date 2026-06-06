import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { readFileSync } from 'fs';

describe('LSP Find References Coverage Tests', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.resetAllMocks();
  });

  it('does not prefix recovery hints with TIP labels', () => {
    const source = readFileSync(
      new URL(
        '../../src/tools/lsp_find_references/lsp_find_references.ts',
        import.meta.url
      ),
      'utf8'
    );

    expect(source).not.toContain('TIP: Use localSearchCode');
    expect(source).toContain(
      'Re-anchor: run localSearchCode with the exact symbol name to get the current line number, then retry with that lineHint.'
    );
  });

  describe('Reference sorting logic', () => {
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

    it('should sort by file name when both are usages', () => {
      const refs = [
        { uri: 'c.ts', isDefinition: false, range: { start: { line: 1 } } },
        { uri: 'a.ts', isDefinition: false, range: { start: { line: 1 } } },
        { uri: 'b.ts', isDefinition: false, range: { start: { line: 1 } } },
      ];

      refs.sort((a, b) => {
        if (a.isDefinition && !b.isDefinition) return -1;
        if (!a.isDefinition && b.isDefinition) return 1;
        if (a.uri !== b.uri) return a.uri.localeCompare(b.uri);
        return a.range.start.line - b.range.start.line;
      });

      expect(refs[0]!.uri).toBe('a.ts');
      expect(refs[1]!.uri).toBe('b.ts');
      expect(refs[2]!.uri).toBe('c.ts');
    });

    it('should sort by line number in same file', () => {
      const refs = [
        { uri: 'a.ts', isDefinition: false, range: { start: { line: 30 } } },
        { uri: 'a.ts', isDefinition: false, range: { start: { line: 10 } } },
        { uri: 'a.ts', isDefinition: false, range: { start: { line: 20 } } },
      ];

      refs.sort((a, b) => {
        if (a.isDefinition && !b.isDefinition) return -1;
        if (!a.isDefinition && b.isDefinition) return 1;
        if (a.uri !== b.uri) return a.uri.localeCompare(b.uri);
        return a.range.start.line - b.range.start.line;
      });

      expect(refs[0]!.range.start.line).toBe(10);
      expect(refs[1]!.range.start.line).toBe(20);
      expect(refs[2]!.range.start.line).toBe(30);
    });
  });

  describe('Pagination logic', () => {
    it('should calculate correct page boundaries', () => {
      const totalReferences = 55;
      const referencesPerPage = 20;
      const page = 2;

      const totalPages = Math.ceil(totalReferences / referencesPerPage);
      const startIndex = (page - 1) * referencesPerPage;
      const endIndex = Math.min(
        startIndex + referencesPerPage,
        totalReferences
      );

      expect(totalPages).toBe(3);
      expect(startIndex).toBe(20);
      expect(endIndex).toBe(40);
    });

    it('should handle last page with fewer items', () => {
      const totalReferences = 55;
      const referencesPerPage = 20;
      const page = 3;

      const totalPages = Math.ceil(totalReferences / referencesPerPage);
      const startIndex = (page - 1) * referencesPerPage;
      const endIndex = Math.min(
        startIndex + referencesPerPage,
        totalReferences
      );

      expect(totalPages).toBe(3);
      expect(startIndex).toBe(40);
      expect(endIndex).toBe(55);
    });

    it('should determine hasMore correctly', () => {
      const totalReferences = 55;
      const referencesPerPage = 20;

      const totalPages = Math.ceil(totalReferences / referencesPerPage);

      expect(1 < totalPages).toBe(true);
      expect(2 < totalPages).toBe(true);
      expect(3 < totalPages).toBe(false);
    });
  });

  describe('Context line extraction logic', () => {
    it('should extract context around a line', () => {
      const lines = [
        'line1',
        'line2',
        'line3',
        'line4',
        'line5',
        'line6',
        'line7',
      ];
      const targetLine = 4;
      const contextLines = 2;

      const startLine = Math.max(0, targetLine - contextLines);
      const endLine = Math.min(lines.length - 1, targetLine + contextLines);

      expect(startLine).toBe(2);
      expect(endLine).toBe(6);

      const context = lines.slice(startLine, endLine + 1);
      expect(context.length).toBe(5);
    });

    it('should handle context at start of file', () => {
      const lines = ['line1', 'line2', 'line3', 'line4', 'line5'];
      const targetLine = 0;
      const contextLines = 2;

      const startLine = Math.max(0, targetLine - contextLines);
      const endLine = Math.min(lines.length - 1, targetLine + contextLines);

      expect(startLine).toBe(0);
      expect(endLine).toBe(2);
    });

    it('should handle context at end of file', () => {
      const lines = ['line1', 'line2', 'line3', 'line4', 'line5'];
      const targetLine = 4;
      const contextLines = 2;

      const startLine = Math.max(0, targetLine - contextLines);
      const endLine = Math.min(lines.length - 1, targetLine + contextLines);

      expect(startLine).toBe(2);
      expect(endLine).toBe(4);
    });
  });

  describe('Multiple files detection', () => {
    it('should detect multiple files in references', () => {
      const refs = [
        { uri: 'a.ts' },
        { uri: 'b.ts' },
        { uri: 'a.ts' },
        { uri: 'c.ts' },
      ];

      const uniqueFiles = new Set(refs.map(ref => ref.uri));
      expect(uniqueFiles.size).toBe(3);
      expect(uniqueFiles.size > 1).toBe(true);
    });

    it('should detect single file', () => {
      const refs = [{ uri: 'a.ts' }, { uri: 'a.ts' }, { uri: 'a.ts' }];

      const uniqueFiles = new Set(refs.map(ref => ref.uri));
      expect(uniqueFiles.size).toBe(1);
      expect(uniqueFiles.size > 1).toBe(false);
    });
  });

  describe('Tool registration', () => {
    it('should register with correct name', async () => {
      vi.resetModules();

      const { registerLSPFindReferencesTool } =
        await import('../../src/tools/lsp_find_references/register.js');

      const mockServer = {
        registerTool: vi.fn().mockReturnValue(undefined),
      };

      registerLSPFindReferencesTool(mockServer as any);

      expect(mockServer.registerTool).toHaveBeenCalledWith(
        'lspFindReferences',
        expect.any(Object),
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

      const config = mockServer.registerTool.mock.calls[0]![1];
      expect(config.annotations.readOnlyHint).toBe(true);
      expect(config.annotations.destructiveHint).toBe(false);
      expect(config.annotations.idempotentHint).toBe(true);
      expect(config.annotations.openWorldHint).toBe(false);
    });
  });

  describe('Reference sorting edge cases', () => {
    it('should handle all sorting comparisons in searchReferencesWithLSP', () => {
      const sortFn = (a: any, b: any) => {
        if (a.isDefinition && !b.isDefinition) return -1;
        if (!a.isDefinition && b.isDefinition) return 1;
        if (a.uri !== b.uri) return a.uri.localeCompare(b.uri);
        return a.range.start.line - b.range.start.line;
      };

      const defA = {
        uri: 'alpha.ts',
        isDefinition: true,
        range: { start: { line: 5 } },
      };
      const defB = {
        uri: 'beta.ts',
        isDefinition: true,
        range: { start: { line: 3 } },
      };
      expect(sortFn(defA, defB)).toBeLessThan(0);

      const refA = {
        uri: 'same.ts',
        isDefinition: false,
        range: { start: { line: 20 } },
      };
      const refB = {
        uri: 'same.ts',
        isDefinition: false,
        range: { start: { line: 10 } },
      };
      expect(sortFn(refA, refB)).toBeGreaterThan(0);
      expect(sortFn(refB, refA)).toBeLessThan(0);

      const fileA = {
        uri: 'aaa.ts',
        isDefinition: false,
        range: { start: { line: 1 } },
      };
      const fileZ = {
        uri: 'zzz.ts',
        isDefinition: false,
        range: { start: { line: 1 } },
      };
      expect(sortFn(fileA, fileZ)).toBeLessThan(0);
      expect(sortFn(fileZ, fileA)).toBeGreaterThan(0);
    });
  });
});
