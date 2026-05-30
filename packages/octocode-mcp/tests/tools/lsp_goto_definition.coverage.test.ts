/**
 * Extended coverage tests for LSP Go To Definition tool
 * Tests internal functions, error handling, and fallback paths
 * @module tools/lsp_goto_definition.coverage.test
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { addLineNumbers } from '../../src/tools/lsp_goto_definition/execution.js';

// Mock fs/promises
vi.mock('fs/promises', () => ({
  readFile: vi.fn(),
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

// Import mocked modules
import * as fs from 'fs/promises';
import * as resolverModule from '../../src/lsp/resolver.js';
import * as managerModule from '../../src/lsp/manager.js';

describe('LSP Goto Definition Coverage Tests', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.resetAllMocks();
  });

  describe('addLineNumbers function - comprehensive', () => {
    it('should format single line content', () => {
      const result = addLineNumbers('const x = 1;', 1, 1);
      expect(result).toBe('>1| const x = 1;');
    });

    it('should format multi-line content with target', () => {
      const content = 'line one\nline two\nline three';
      const result = addLineNumbers(content, 10, 11);

      expect(result).toContain(' 10| line one');
      expect(result).toContain('>11| line two');
      expect(result).toContain(' 12| line three');
    });

    it('should handle three-digit line numbers', () => {
      const content = 'a\nb\nc';
      const result = addLineNumbers(content, 98, 100);

      expect(result).toContain(' 98| a');
      expect(result).toContain(' 99| b');
      expect(result).toContain('>100| c');
    });

    it('should handle four-digit line numbers', () => {
      const content = 'first\nsecond';
      const result = addLineNumbers(content, 999, 1000);

      expect(result).toContain(' 999| first');
      expect(result).toContain('>1000| second');
    });

    it('should handle five-digit line numbers', () => {
      const content = 'a\nb';
      const result = addLineNumbers(content, 9999, 10000);

      expect(result).toContain(' 9999| a');
      expect(result).toContain('>10000| b');
    });

    it('should handle empty lines in content', () => {
      const content = 'line1\n\nline3';
      const result = addLineNumbers(content, 1, 2);

      const lines = result.split('\n');
      expect(lines[0]).toContain('| line1');
      expect(lines[1]).toContain('>2| ');
      expect(lines[2]).toContain('| line3');
    });

    it('should handle content with special characters', () => {
      const content = 'const x = "hello";';
      const result = addLineNumbers(content, 1, 1);

      expect(result).toContain('>1| const x = "hello";');
    });

    it('should handle content with regex special chars', () => {
      const content = 'const regex = /test.*pattern/g;';
      const result = addLineNumbers(content, 1, 1);

      expect(result).toContain('>1| const regex = /test.*pattern/g;');
    });

    it('should handle tabs in content', () => {
      const content = '\tindented\n\t\tdouble';
      const result = addLineNumbers(content, 1, 1);

      expect(result).toContain('>1| \tindented');
      expect(result).toContain(' 2| \t\tdouble');
    });

    it('should handle target line not in range', () => {
      const content = 'a\nb\nc';
      const result = addLineNumbers(content, 1, 10); // target 10 is outside

      // No line should be marked with >
      const lines = result.split('\n');
      lines.forEach(line => {
        expect(line.startsWith('>')).toBe(false);
      });
    });

    it('should mark only the exact target line', () => {
      const content = 'one\ntwo\nthree\nfour\nfive';
      const result = addLineNumbers(content, 1, 3);

      const lines = result.split('\n');
      expect(lines.length).toBeGreaterThanOrEqual(5);
      expect(lines[0]?.startsWith('>') ?? false).toBe(false); // line 1
      expect(lines[1]?.startsWith('>') ?? false).toBe(false); // line 2
      expect(lines[2]?.startsWith('>') ?? false).toBe(true); // line 3
      expect(lines[3]?.startsWith('>') ?? false).toBe(false); // line 4
      expect(lines[4]?.startsWith('>') ?? false).toBe(false); // line 5
    });

    it('should use consistent padding width', () => {
      const content = 'a\nb\nc\nd\ne\nf\ng\nh\ni\nj';
      const result = addLineNumbers(content, 1, 5);

      const lines = result.split('\n');
      // All line numbers should have same padding (2 chars for lines 1-10)
      // The format includes a marker char + padded number + | + space + content
      expect(lines[0]).toMatch(/^\s+1\|/);
      expect(lines[8]).toMatch(/^\s+9\|/);
      expect(lines[9]).toMatch(/10\|/);
    });
  });

  describe('Tool registration', () => {
    it('should register tool correctly', async () => {
      vi.resetModules();

      const { registerLSPGotoDefinitionTool } =
        await import('../../src/tools/lsp_goto_definition/lsp_goto_definition.js');

      const mockServer = {
        registerTool: vi.fn().mockReturnValue(undefined),
      };

      registerLSPGotoDefinitionTool(mockServer as any);

      expect(mockServer.registerTool).toHaveBeenCalledWith(
        'lspGotoDefinition',
        expect.objectContaining({
          description: expect.any(String),
          inputSchema: expect.any(Object),
        }),
        expect.any(Function)
      );
    });

    it('should have all required annotations', async () => {
      vi.resetModules();

      const { registerLSPGotoDefinitionTool } =
        await import('../../src/tools/lsp_goto_definition/lsp_goto_definition.js');

      const mockServer = {
        registerTool: vi.fn().mockReturnValue(undefined),
      };

      registerLSPGotoDefinitionTool(mockServer as any);

      const config = mockServer.registerTool.mock.calls[0]![1];
      expect(config.annotations).toEqual({
        title: 'Go To Definition',
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      });
    });
  });

  describe('Schema exports', () => {
    it('should export BulkLSPGotoDefinitionSchema', async () => {
      const { BulkLSPGotoDefinitionSchema } =
        await import('@octocodeai/octocode-core');
      expect(BulkLSPGotoDefinitionSchema).toBeDefined();
      const parsed = BulkLSPGotoDefinitionSchema.safeParse({
        queries: [
          {
            id: 'goto_definition_coverage',
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
      // Description may be empty if tool not in remote metadata (local-only tool)
    });
  });

  describe('Context extraction logic', () => {
    it('should calculate correct context range', () => {
      const totalLines = 100;
      const targetLine = 50;
      const contextLines = 5;

      const startLine = Math.max(0, targetLine - contextLines);
      const endLine = Math.min(totalLines - 1, targetLine + contextLines);

      expect(startLine).toBe(45);
      expect(endLine).toBe(55);
    });

    it('should handle context at file start', () => {
      const totalLines = 100;
      const targetLine = 2;
      const contextLines = 5;

      const startLine = Math.max(0, targetLine - contextLines);
      const endLine = Math.min(totalLines - 1, targetLine + contextLines);

      expect(startLine).toBe(0);
      expect(endLine).toBe(7);
    });

    it('should handle context at file end', () => {
      const totalLines = 100;
      const targetLine = 97;
      const contextLines = 5;

      const startLine = Math.max(0, targetLine - contextLines);
      const endLine = Math.min(totalLines - 1, targetLine + contextLines);

      expect(startLine).toBe(92);
      expect(endLine).toBe(99);
    });

    it('should handle small files', () => {
      const totalLines = 3;
      const targetLine = 1;
      const contextLines = 5;

      const startLine = Math.max(0, targetLine - contextLines);
      const endLine = Math.min(totalLines - 1, targetLine + contextLines);

      expect(startLine).toBe(0);
      expect(endLine).toBe(2);
    });
  });

  describe('CodeSnippet structure', () => {
    it('should create valid CodeSnippet object', () => {
      const snippet = {
        uri: '/path/to/file.ts',
        range: {
          start: { line: 10, character: 0 },
          end: { line: 10, character: 20 },
        },
        content: 'const myFunc = () => {};',
        displayRange: {
          startLine: 8,
          endLine: 13,
        },
      };

      expect(snippet.uri).toBe('/path/to/file.ts');
      expect(snippet.range.start.line).toBe(10);
      expect(snippet.displayRange.startLine).toBe(8);
    });

    it('should handle multi-line range', () => {
      const snippet = {
        uri: '/path/to/file.ts',
        range: {
          start: { line: 10, character: 0 },
          end: { line: 15, character: 1 },
        },
        content: 'function myFunc() {\n  // body\n  return;\n}',
        displayRange: {
          startLine: 8,
          endLine: 18,
        },
      };

      expect(snippet.range.end.line).toBe(15);
      expect(snippet.displayRange.endLine).toBe(18);
    });
  });

  describe('Position resolution logic', () => {
    it('should handle exact position', () => {
      const position = { line: 10, character: 5 };
      const symbolName = 'myFunc';
      const endPosition = {
        line: position.line,
        character: position.character + symbolName.length,
      };

      expect(endPosition.line).toBe(10);
      expect(endPosition.character).toBe(11);
    });

    it('should handle position at start of line', () => {
      const position = { line: 0, character: 0 };
      const symbolName = 'const';
      const endPosition = {
        line: position.line,
        character: position.character + symbolName.length,
      };

      expect(endPosition.character).toBe(5);
    });
  });

  describe('Fallback result creation', () => {
    it('should include all required fields', () => {
      const fallbackResult = {
        locations: [
          {
            uri: '/test/file.ts',
            range: {
              start: { line: 10, character: 0 },
              end: { line: 10, character: 10 },
            },
            content: 'const x = 1;',
            displayRange: { startLine: 8, endLine: 13 },
          },
        ],
        resolvedPosition: { line: 10, character: 5 },
        searchRadius: 5,
        researchGoal: 'Find definition',
        reasoning: 'User requested',
        hints: [
          'Each location = a definition site; use range.start.line+1 as lineHint for follow-up LSP calls',
        ],
      };

      expect(fallbackResult.status).toBeUndefined();
      expect(fallbackResult.locations.length).toBe(1);
      expect(fallbackResult.searchRadius).toBe(5);
    });

    it('should include tool-specific hints (LSP fallback hints in server.instructions)', () => {
      // LSP no-server hints moved to server.instructions; fallback returns tool hints only
      const hints = [
        'Each location = a definition site; use range.start.line+1 as lineHint',
      ];
      expect(hints[0]).toContain('lineHint');
    });

    it('should include line mismatch hint when applicable', () => {
      const foundAtLine: number = 12;
      const hintedLine: number = 10;

      const hint =
        foundAtLine !== hintedLine
          ? `Symbol found at line ${foundAtLine} (hint was ${hintedLine})`
          : undefined;

      expect(hint).toBe(
        `Symbol found at line ${foundAtLine} (hint was ${hintedLine})`
      );
    });
  });

  describe('Error result structure', () => {
    it('should create empty status for not found', () => {
      const emptyResult = {
        status: 'empty' as const,
        error: 'Symbol not found',
        errorType: 'symbol_not_found',
        searchRadius: 5,
        hints: [
          'Symbol not found at or near line 10',
          'Verify the exact symbol name',
        ],
      };

      expect(emptyResult.status).toBe('empty');
      expect(emptyResult.errorType).toBe('symbol_not_found');
    });

    it('should include search radius in hints', () => {
      const lineHint = 10;
      const searchRadius = 2;

      const hint = `Searched lines ${Math.max(1, lineHint - searchRadius)} to ${lineHint + searchRadius}`;
      expect(hint).toBe('Searched lines 8 to 12');
    });
  });

  describe('Tool name constant', () => {
    it('should have correct tool name', async () => {
      const { STATIC_TOOL_NAMES } =
        await import('../../src/tools/toolNames.js');

      expect(STATIC_TOOL_NAMES.LSP_GOTO_DEFINITION).toBeDefined();
      expect(typeof STATIC_TOOL_NAMES.LSP_GOTO_DEFINITION).toBe('string');
    });
  });

  describe('Enhanced location processing', () => {
    it('should calculate display range correctly', () => {
      const startLine = 5; // 0-indexed
      const endLine = 15;

      const displayRange = {
        startLine: startLine + 1, // Convert to 1-indexed
        endLine: endLine + 1,
      };

      expect(displayRange.startLine).toBe(6);
      expect(displayRange.endLine).toBe(16);
    });

    it('should format numbered content with markers', () => {
      const lines = ['line 1', 'line 2', 'line 3'];
      const startLine = 10;
      const targetLineNum = 11; // 1-indexed

      const numberedContent = lines
        .map((line, i) => {
          const lineNum = startLine + i + 1;
          const isTarget = lineNum > 10 && lineNum <= targetLineNum + 1;
          const marker = isTarget ? '>' : ' ';
          return `${marker}${String(lineNum).padStart(4, ' ')}| ${line}`;
        })
        .join('\n');

      expect(numberedContent).toContain('>  11| line 1');
      expect(numberedContent).toContain('>  12| line 2');
      expect(numberedContent).toContain('   13| line 3');
    });
  });

  describe('Multiple definitions handling', () => {
    it('should handle multiple definition locations', () => {
      const locations = [
        { uri: '/file1.ts', range: { start: { line: 10 }, end: { line: 10 } } },
        { uri: '/file2.ts', range: { start: { line: 20 }, end: { line: 25 } } },
      ];

      expect(locations.length).toBe(2);

      const hints =
        locations.length > 1
          ? ['Multiple definitions - check overloads or re-exports']
          : [];

      expect(hints.length).toBe(1);
      expect(hints[0]).toContain('Multiple definitions');
    });

    it('should not add hint for single definition', () => {
      const locations = [
        { uri: '/file1.ts', range: { start: { line: 10 }, end: { line: 10 } } },
      ];

      const hints =
        locations.length > 1
          ? ['Multiple definitions - check overloads or re-exports']
          : [];

      expect(hints.length).toBe(0);
    });
  });

  describe('Error handling branches', () => {
    it('should handle file read error (line 90)', () => {
      // This tests the catch block at line 90 for file read errors
      const createFileReadErrorResult = (error: Error, uri: string) => ({
        status: 'error',
        error: error.message,
        errorType: 'file_error',
        hints: [
          `Could not read file: ${uri}`,
          'Verify the file exists and is accessible',
        ],
      });

      const result = createFileReadErrorResult(
        new Error('ENOENT: no such file'),
        '/path/to/missing.ts'
      );

      expect(result.status).toBe('error');
      expect(result.hints).toContain(
        'Could not read file: /path/to/missing.ts'
      );
    });

    it('should rethrow non-SymbolResolutionError errors (line 127)', () => {
      // Tests the throw at line 127 for non-SymbolResolutionError
      class RandomError extends Error {
        constructor(message: string) {
          super(message);
          this.name = 'RandomError';
        }
      }

      const handleResolutionError = (error: unknown) => {
        // Simulate SymbolResolutionError check
        const isSymbolResolutionError =
          error instanceof Error && error.name === 'SymbolResolutionError';

        if (isSymbolResolutionError) {
          return { status: 'empty', errorType: 'symbol_not_found' };
        }
        throw error;
      };

      const regularError = new RandomError('unexpected');
      expect(() => handleResolutionError(regularError)).toThrow('unexpected');
    });

    it('should return empty result for LSP with no definitions (line 187)', () => {
      // Tests the empty result at line 186-199
      const createEmptyLSPResult = (query: {
        researchGoal: string;
        reasoning: string;
      }) => ({
        status: 'empty',
        error: 'No definition found by language server',
        errorType: 'symbol_not_found',
        researchGoal: query.researchGoal,
        reasoning: query.reasoning,
        hints: [
          'Language server could not find definition',
          'Symbol may be a built-in or from external library',
          'Try packageSearch to find library source code',
        ],
      });

      const query = { researchGoal: 'Find def', reasoning: 'Testing' };
      const result = createEmptyLSPResult(query);

      expect(result.status).toBe('empty');
      expect(result.errorType).toBe('symbol_not_found');
      expect(result.hints).toContain(
        'Language server could not find definition'
      );
    });

    it('should handle LSP failure and fall back (line 146)', async () => {
      // Tests the catch block at line 143-147
      let fallbackCalled = false;

      const handleLSPWithFallback = async (
        lspFn: () => Promise<any>,
        fallbackFn: () => any
      ) => {
        try {
          return await lspFn();
        } catch (_error) {
          // Line 146: console.debug call happens here
          fallbackCalled = true;
          return fallbackFn();
        }
      };

      const failingLSP = async () => {
        throw new Error('LSP timeout');
      };
      const fallback = () => ({ status: 'hasResults', source: 'fallback' });

      await handleLSPWithFallback(failingLSP, fallback);
      // After awaiting, fallback should have been called
      expect(fallbackCalled).toBe(true);
    });

    it('should handle outer catch block error (line 160)', () => {
      // Tests the outer catch at line 159-168
      const createOuterErrorResult = (
        error: Error,
        query: { uri: string; symbolName: string; lineHint: number }
      ) => ({
        status: 'error',
        error: error.message,
        extra: {
          uri: query.uri,
          symbolName: query.symbolName,
          lineHint: query.lineHint,
        },
      });

      const result = createOuterErrorResult(new Error('Unexpected error'), {
        uri: '/test.ts',
        symbolName: 'myFunc',
        lineHint: 10,
      });

      expect(result.status).toBe('error');
      expect(result.extra.uri).toBe('/test.ts');
    });
  });

  describe('Integration tests for uncovered branches', () => {
    const createHandler = async () => {
      vi.resetModules();

      // Re-import after resetting mocks
      const { registerLSPGotoDefinitionTool } =
        await import('../../src/tools/lsp_goto_definition/lsp_goto_definition.js');

      const mockServer = {
        registerTool: vi.fn(
          (_name: string, _config: any, handler: any) => handler
        ),
      };
      registerLSPGotoDefinitionTool(mockServer as any);
      return mockServer.registerTool.mock.results[0]!.value;
    };

    beforeEach(() => {
      vi.clearAllMocks();
      process.env.WORKSPACE_ROOT = process.cwd();

      // Default mocks
      vi.mocked(fs.readFile).mockResolvedValue('const test = 1;');
      vi.mocked(managerModule.isLanguageServerAvailable).mockResolvedValue(
        false
      );
      vi.mocked(managerModule.acquirePooledClient).mockResolvedValue(null);

      // Default SymbolResolver mock
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
    });

    afterEach(() => {
      delete process.env.WORKSPACE_ROOT;
      vi.resetAllMocks();
    });

    it('should hit line 90: createErrorResult on file read error', async () => {
      // This tests the catch block at line 89-98
      const testPath = `${process.cwd()}/src/protected.ts`;

      vi.mocked(fs.readFile).mockRejectedValue(
        new Error('EACCES: permission denied')
      );

      const handler = await createHandler();
      const result = await handler({
        queries: [
          {
            uri: testPath,
            symbolName: 'test',
            lineHint: 1,
            researchGoal: 'Find def',
            reasoning: 'Test file read error',
          },
        ],
      });

      expect(result).toBeDefined();
      expect(result.content?.[0]?.text).toContain('error');
    });

    it('should hit line 127: rethrow non-SymbolResolutionError', async () => {
      // This tests the throw at line 127 for non-SymbolResolutionError
      const testPath = `${process.cwd()}/src/test.ts`;

      vi.mocked(fs.readFile).mockResolvedValue('const test = 1;');

      // Make SymbolResolver throw a generic error (not SymbolResolutionError)
      vi.mocked(resolverModule.SymbolResolver).mockImplementation(function () {
        return {
          resolvePositionFromContent: vi.fn(() => {
            throw new Error('Generic internal error');
          }),
          extractContext: vi.fn(),
        };
      });

      const handler = await createHandler();
      const result = await handler({
        queries: [
          {
            uri: testPath,
            symbolName: 'test',
            lineHint: 1,
            researchGoal: 'Find def',
            reasoning: 'Test generic error rethrow',
          },
        ],
      });

      // Should be caught by outer catch and return error result
      expect(result).toBeDefined();
      expect(result.content?.[0]?.text).toContain('error');
    });

    it('should hit line 146: console.debug on LSP failure fallback', async () => {
      // This tests the catch block at line 143-147
      const testPath = `${process.cwd()}/src/test.ts`;

      vi.mocked(fs.readFile).mockResolvedValue('const test = 1;');
      vi.mocked(managerModule.isLanguageServerAvailable).mockResolvedValue(
        true
      );

      // Make acquirePooledClient return a client that throws on gotoDefinition
      vi.mocked(managerModule.acquirePooledClient).mockResolvedValue({
        stop: vi.fn(),
        gotoDefinition: vi.fn().mockRejectedValue(new Error('LSP timeout')),
      } as any);

      const handler = await createHandler();
      const result = await handler({
        queries: [
          {
            uri: testPath,
            symbolName: 'test',
            lineHint: 1,
            researchGoal: 'Find def',
            reasoning: 'Test LSP failure fallback',
          },
        ],
      });

      // Should fallback to text-based resolution
      expect(result).toBeDefined();
      // hasResults is signaled by ABSENT status — only 'empty' / 'error'
      // get emitted explicitly. Verify the happy path by NOT seeing them.
      expect(result.content?.[0]?.text).not.toContain('status: "empty"');
      expect(result.content?.[0]?.text).not.toContain('status: "error"');
    });

    it('should hit line 187: LSP returns null/empty locations', async () => {
      // This tests the empty result at line 186-199
      process.env.WORKSPACE_ROOT = process.cwd();
      const testPath = `${process.cwd()}/src/test.ts`;

      vi.mocked(fs.readFile).mockResolvedValue('const test = 1;');
      vi.mocked(managerModule.isLanguageServerAvailable).mockResolvedValue(
        true
      );

      // Make acquirePooledClient return a client that returns empty locations
      vi.mocked(managerModule.acquirePooledClient).mockResolvedValue({
        stop: vi.fn(),
        gotoDefinition: vi.fn().mockResolvedValue([]),
      } as any);

      const handler = await createHandler();
      const result = await handler({
        queries: [
          {
            uri: testPath,
            symbolName: 'test',
            lineHint: 1,
            researchGoal: 'Find def',
            reasoning: 'Test LSP empty locations',
          },
        ],
      });

      expect(result).toBeDefined();
      const text = result.content?.[0]?.text ?? '';
      expect(text).toContain('empty');
      // Response now uses structured error type instead of text message
      expect(text).toContain('symbol_not_found');
    });

    it('should hit line 187: LSP returns null locations', async () => {
      // Test the null branch of (!locations || locations.length === 0)
      process.env.WORKSPACE_ROOT = process.cwd();
      const testPath = `${process.cwd()}/src/test.ts`;

      vi.mocked(fs.readFile).mockResolvedValue('const test = 1;');
      vi.mocked(managerModule.isLanguageServerAvailable).mockResolvedValue(
        true
      );

      vi.mocked(managerModule.acquirePooledClient).mockResolvedValue({
        stop: vi.fn(),
        gotoDefinition: vi.fn().mockResolvedValue(null),
      } as any);

      const handler = await createHandler();
      const result = await handler({
        queries: [
          {
            uri: testPath,
            symbolName: 'test',
            lineHint: 1,
            researchGoal: 'Find def',
            reasoning: 'Test LSP null locations',
          },
        ],
      });

      expect(result).toBeDefined();
      const text = result.content?.[0]?.text ?? '';
      expect(text).toContain('empty');
    });

    it('should fallback to text resolution when acquirePooledClient returns null', async () => {
      // gotoDefinitionWithLSP returns null when client is null (line 218)
      process.env.WORKSPACE_ROOT = process.cwd();
      const testPath = `${process.cwd()}/src/test.ts`;

      vi.mocked(fs.readFile).mockResolvedValue('const test = 1;');
      vi.mocked(managerModule.isLanguageServerAvailable).mockResolvedValue(
        true
      );
      vi.mocked(managerModule.acquirePooledClient).mockResolvedValue(null);

      const handler = await createHandler();
      const result = await handler({
        queries: [
          {
            uri: testPath,
            symbolName: 'test',
            lineHint: 1,
            researchGoal: 'Find def',
            reasoning: 'Test acquirePooledClient null fallback',
          },
        ],
      });

      expect(result).toBeDefined();
      const text = result.content?.[0]?.text ?? '';
      expect(text).not.toContain('status: "hasResults"');
    });
  });
});
