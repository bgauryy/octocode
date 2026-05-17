/**
 * Extended coverage tests for LSP Call Hierarchy tool
 * Tests execution flow and internal helpers
 * @module tools/lsp_call_hierarchy.coverage.test
 */

import { describe, it, expect, vi, beforeEach, afterEach, Mock } from 'vitest';
import { registerLSPCallHierarchyTool } from '../../src/tools/lsp_call_hierarchy/register.js';
import {
  parseRipgrepJsonOutput,
  extractFunctionBody,
  inferSymbolKind,
  createRange,
  isFunctionAssignment,
} from '../../src/tools/lsp_call_hierarchy/callHierarchy.js';
import { SymbolResolver } from '../../src/lsp/resolver.js';
import * as resolverModule from '../../src/lsp/resolver.js';
import * as managerModule from '../../src/lsp/manager.js';
import * as toolHelpers from '../../src/utils/file/toolHelpers.js';
import { safeExec } from '../../src/utils/exec/safe.js';
import { checkCommandAvailability } from '../../src/utils/exec/commandAvailability.js';
import * as fsPromises from 'fs/promises';

// Mocks
vi.mock('fs/promises', () => ({
  readFile: vi.fn(),
}));

vi.mock('../../src/lsp/resolver.js', () => ({
  SymbolResolver: vi.fn(),
  SymbolResolutionError: class extends Error {},
}));

vi.mock('../../src/lsp/manager.js', () => ({
  LSP_UNAVAILABLE_HINT: 'LSP unavailable test',
  acquirePooledClient: vi.fn(),
  isLanguageServerAvailable: vi.fn(),
}));

vi.mock('../../src/utils/file/toolHelpers.js', () => ({
  validateToolPath: vi.fn(),
  createErrorResult: vi.fn(error => ({
    isError: true,
    error: error?.message || String(error),
    status: 'error',
  })),
}));

vi.mock('../../src/utils/exec/safe.js', () => ({
  safeExec: vi.fn(),
}));

vi.mock('../../src/utils/exec/commandAvailability.js', () => ({
  checkCommandAvailability: vi.fn(),
}));

vi.mock('../../src/hints/index.js', () => {
  return {
    getHints: vi.fn(() => []),
  };
});

vi.mock('../../src/utils/response/bulk.js', () => ({
  executeBulkOperation: vi.fn(async (queries, handler) => {
    // Execute handler for each query immediately for testing
    const results = [];
    for (const query of queries) {
      results.push(await handler(query));
    }
    return { content: [{ type: 'text', text: JSON.stringify(results) }] };
  }),
}));

describe('LSP Call Hierarchy Coverage Tests', () => {
  let toolHandler: any;
  let mockServer: any;
  let mockSymbolResolver: any;
  let mockLSPClient: any;

  beforeEach(async () => {
    vi.clearAllMocks();

    mockServer = {
      registerTool: vi.fn((name, schema, handler) => {
        toolHandler = handler;
        return { name, schema, handler };
      }),
    };

    // Setup SymbolResolver mock
    mockSymbolResolver = {
      resolvePositionFromContent: vi.fn(),
    };
    (SymbolResolver as Mock).mockImplementation(function () {
      return mockSymbolResolver;
    });

    // Setup LSP Client mock
    mockLSPClient = {
      stop: vi.fn(),
      prepareCallHierarchy: vi.fn(),
      getIncomingCalls: vi.fn(),
      getOutgoingCalls: vi.fn(),
    };
    (managerModule.acquirePooledClient as Mock).mockResolvedValue(
      mockLSPClient
    );

    // Register tool to get handler
    registerLSPCallHierarchyTool(mockServer);
  });

  afterEach(() => {
    vi.resetAllMocks();
  });

  describe('Execution Flow', () => {
    const baseQuery = {
      uri: '/workspace/file.ts',
      symbolName: 'myFunc',
      lineHint: 10,
      direction: 'incoming',
      researchGoal: 'goal',
      reasoning: 'reason',
      mainResearchGoal: 'main',
    };

    it('should handle invalid path', async () => {
      (toolHelpers.validateToolPath as Mock).mockReturnValue({
        isValid: false,
        errorResult: { isError: true, message: 'Invalid path' },
      });

      const result = await toolHandler({ queries: [baseQuery] });
      const results = JSON.parse(result.content[0].text);
      expect(results[0]).toEqual({ isError: true, message: 'Invalid path' });
    });

    it('should handle file read error', async () => {
      (toolHelpers.validateToolPath as Mock).mockReturnValue({
        isValid: true,
        sanitizedPath: '/workspace/file.ts',
      });
      (fsPromises.readFile as Mock).mockRejectedValue(
        new Error('Access denied')
      );
      (toolHelpers.createErrorResult as Mock).mockReturnValue({
        error: 'Access denied',
      });

      const result = await toolHandler({ queries: [baseQuery] });
      const results = JSON.parse(result.content[0].text);
      expect(toolHelpers.createErrorResult).toHaveBeenCalled();
      expect(results[0]).toEqual({ error: 'Access denied' });
    });

    it('should handle symbol resolution error', async () => {
      (toolHelpers.validateToolPath as Mock).mockReturnValue({
        isValid: true,
        sanitizedPath: '/workspace/file.ts',
      });
      (fsPromises.readFile as Mock).mockResolvedValue('content');

      mockSymbolResolver.resolvePositionFromContent.mockImplementation(() => {
        throw new resolverModule.SymbolResolutionError(
          'myFunc',
          10,
          'Symbol not found'
        );
      });

      const result = await toolHandler({ queries: [baseQuery] });
      const results = JSON.parse(result.content[0].text);
      expect(results[0].status).toBe('empty');
      expect(results[0].errorType).toBe('symbol_not_found');
    });

    describe('LSP Path', () => {
      beforeEach(() => {
        (toolHelpers.validateToolPath as Mock).mockReturnValue({
          isValid: true,
          sanitizedPath: '/workspace/file.ts',
        });
        (fsPromises.readFile as Mock).mockResolvedValue('function myFunc() {}');
        mockSymbolResolver.resolvePositionFromContent.mockReturnValue({
          position: { line: 0, character: 0 },
          foundAtLine: 1,
        });
        (managerModule.isLanguageServerAvailable as Mock).mockResolvedValue(
          true
        );
      });

      it('should use LSP incoming calls when available', async () => {
        mockLSPClient.prepareCallHierarchy.mockResolvedValue([
          {
            name: 'myFunc',
            uri: 'file:///workspace/file.ts',
            range: {
              start: { line: 0, character: 0 },
              end: { line: 0, character: 10 },
            },
            selectionRange: {
              start: { line: 0, character: 0 },
              end: { line: 0, character: 10 },
            },
          },
        ]);

        mockLSPClient.getIncomingCalls.mockResolvedValue([
          {
            from: {
              name: 'caller',
              uri: 'file:///workspace/caller.ts',
              range: {
                start: { line: 0, character: 0 },
                end: { line: 0, character: 10 },
              },
              selectionRange: {
                start: { line: 0, character: 0 },
                end: { line: 0, character: 10 },
              },
            },
            fromRanges: [
              {
                start: { line: 0, character: 0 },
                end: { line: 0, character: 10 },
              },
            ],
          },
        ]);

        const result = await toolHandler({ queries: [baseQuery] });
        const results = JSON.parse(result.content[0].text);

        expect(results[0].status).toBe('hasResults');
        expect(results[0].incomingCalls).toHaveLength(1);
        expect(results[0].incomingCalls[0].from.name).toBe('caller');
      });

      it('should handle empty incoming calls from LSP', async () => {
        mockLSPClient.prepareCallHierarchy.mockResolvedValue([
          {
            name: 'myFunc',
            uri: 'file:///workspace/file.ts',
            range: {
              start: { line: 0, character: 0 },
              end: { line: 0, character: 10 },
            },
            selectionRange: {
              start: { line: 0, character: 0 },
              end: { line: 0, character: 10 },
            },
          },
        ]);
        mockLSPClient.getIncomingCalls.mockResolvedValue([]);

        const result = await toolHandler({ queries: [baseQuery] });
        const results = JSON.parse(result.content[0].text);

        expect(results[0].status).toBe('empty');
        expect(results[0].incomingCalls).toEqual([]);
      });

      it('should use LSP outgoing calls when available', async () => {
        const outgoingQuery = { ...baseQuery, direction: 'outgoing' };

        mockLSPClient.prepareCallHierarchy.mockResolvedValue([
          {
            name: 'myFunc',
            uri: 'file:///workspace/file.ts',
            range: {
              start: { line: 0, character: 0 },
              end: { line: 0, character: 10 },
            },
            selectionRange: {
              start: { line: 0, character: 0 },
              end: { line: 0, character: 10 },
            },
          },
        ]);
        mockLSPClient.getOutgoingCalls.mockResolvedValue([
          {
            to: {
              name: 'callee',
              uri: 'file:///workspace/callee.ts',
              range: {
                start: { line: 0, character: 0 },
                end: { line: 0, character: 10 },
              },
              selectionRange: {
                start: { line: 0, character: 0 },
                end: { line: 0, character: 10 },
              },
            },
            fromRanges: [],
          },
        ]);

        const result = await toolHandler({ queries: [outgoingQuery] });
        const results = JSON.parse(result.content[0].text);

        expect(results[0].status).toBe('hasResults');
        expect(results[0].outgoingCalls).toHaveLength(1);
        expect(results[0].outgoingCalls[0].to.name).toBe('callee');
      });

      it('should return empty status when LSP outgoing calls returns empty (line 166)', async () => {
        const outgoingQuery = { ...baseQuery, direction: 'outgoing' };

        mockLSPClient.prepareCallHierarchy.mockResolvedValue([
          {
            name: 'myFunc',
            uri: 'file:///workspace/file.ts',
            range: {
              start: { line: 0, character: 0 },
              end: { line: 0, character: 10 },
            },
            selectionRange: {
              start: { line: 0, character: 0 },
              end: { line: 0, character: 10 },
            },
          },
        ]);
        mockLSPClient.getOutgoingCalls.mockResolvedValue([]);

        const result = await toolHandler({ queries: [outgoingQuery] });
        const results = JSON.parse(result.content[0].text);

        expect(results[0].status).toBe('empty');
        expect(results[0].outgoingCalls).toEqual([]);
        expect(results[0].direction).toBe('outgoing');
      });

      it('should handle LSP prepareCallHierarchy returning empty', async () => {
        mockLSPClient.prepareCallHierarchy.mockResolvedValue([]);

        const result = await toolHandler({ queries: [baseQuery] });
        const results = JSON.parse(result.content[0].text);

        expect(results[0].status).toBe('empty');
        expect(results[0].error).toContain('No callable symbol found');
      });

      it('should fallback to pattern matching if LSP throws', async () => {
        mockLSPClient.prepareCallHierarchy.mockRejectedValue(
          new Error('LSP error')
        );

        // Setup fallback mocks
        (checkCommandAvailability as Mock).mockResolvedValue({
          available: false,
        });
        (safeExec as Mock).mockResolvedValue({
          success: true,
          stdout: '',
        }); // Grep returns nothing

        const result = await toolHandler({ queries: [baseQuery] });
        const results = JSON.parse(result.content[0].text);

        // Should have tried fallback (grep/rg)
        expect(safeExec).toHaveBeenCalled();
        // Since grep returned empty
        expect(results[0].status).toBe('empty');
        expect(results[0].hints.length).toBeGreaterThan(0);
      });
    });

    describe('Pattern Matching Fallback', () => {
      beforeEach(() => {
        (toolHelpers.validateToolPath as Mock).mockReturnValue({
          isValid: true,
          sanitizedPath: '/workspace/file.ts',
        });
        (fsPromises.readFile as Mock).mockResolvedValue('function myFunc() {}');
        mockSymbolResolver.resolvePositionFromContent.mockReturnValue({
          position: { line: 0, character: 0 },
          foundAtLine: 1,
        });
        (managerModule.isLanguageServerAvailable as Mock).mockResolvedValue(
          false
        );
      });

      it('should use ripgrep for incoming calls', async () => {
        (safeExec as Mock).mockResolvedValue({
          success: true,
          stdout: JSON.stringify({
            type: 'match',
            data: {
              path: { text: '/workspace/caller.ts' },
              line_number: 5,
              lines: { text: 'myFunc();' },
              submatches: [{ start: 0 }],
            },
          }),
        });

        const result = await toolHandler({ queries: [baseQuery] });
        const results = JSON.parse(result.content[0].text);

        expect(safeExec).toHaveBeenCalledWith(
          expect.stringMatching(/(^|\/)rg$/),
          expect.any(Array),
          expect.any(Object)
        );
        expect(results[0].status).toBe('hasResults');
        expect(results[0].incomingCalls).toHaveLength(1);
      });

      it('should handle search errors', async () => {
        (safeExec as Mock).mockResolvedValue({
          success: false,
          code: 2,
          stderr: 'Search failed',
        });

        const result = await toolHandler({ queries: [baseQuery] });
        const results = JSON.parse(result.content[0].text);

        expect(results[0].status).toBe('error');
        expect(results[0].error).toContain('Search failed');
      });

      it('should find outgoing calls by parsing body', async () => {
        const outgoingQuery = { ...baseQuery, direction: 'outgoing' };
        const content = `
                function myFunc() {
                    otherFunc();
                }
            `;
        (fsPromises.readFile as Mock).mockResolvedValue(content);
        mockSymbolResolver.resolvePositionFromContent.mockReturnValue({
          position: { line: 1, character: 16 },
          foundAtLine: 2,
        });

        const result = await toolHandler({ queries: [outgoingQuery] });
        const results = JSON.parse(result.content[0].text);

        expect(results[0].status).toBe('hasResults');
        expect(results[0].outgoingCalls[0].to.name).toBe('otherFunc');
      });

      it('should handle failure to extract function body', async () => {
        const outgoingQuery = { ...baseQuery, direction: 'outgoing' };
        const content = 'function myFunc()'; // No body
        (fsPromises.readFile as Mock).mockResolvedValue(content);

        const result = await toolHandler({ queries: [outgoingQuery] });
        const results = JSON.parse(result.content[0].text);

        expect(results[0].status).toBe('empty');
        expect(results[0].hints).toContain('Could not extract function body');
      });

      it('should return empty when function body has no outgoing calls (uniqueCalls.length === 0)', async () => {
        const outgoingQuery = { ...baseQuery, direction: 'outgoing' };
        const content = `function myFunc() {
  return 1 + 2;
}`;
        (fsPromises.readFile as Mock).mockResolvedValue(content);
        mockSymbolResolver.resolvePositionFromContent.mockReturnValue({
          position: { line: 1, character: 16 },
          foundAtLine: 2,
        });

        const result = await toolHandler({ queries: [outgoingQuery] });
        const results = JSON.parse(result.content[0].text);

        expect(results[0].status).toBe('empty');
        expect(results[0].outgoingCalls).toEqual([]);
        expect(results[0].hints?.length).toBeGreaterThan(0);
      });

      it('should report search error from ripgrep failure (code !== 1)', async () => {
        (safeExec as Mock).mockResolvedValue({
          success: false,
          code: 2,
          stderr: 'rg: invalid option',
        });

        const result = await toolHandler({ queries: [baseQuery] });
        const results = JSON.parse(result.content[0].text);

        expect(results[0].status).toBe('error');
        expect(results[0].error).toBeDefined();
      });
    });
  });

  // Unit tests for helper functions
  describe('parseRipgrepJsonOutput', () => {
    it('should parse valid match', () => {
      const output = JSON.stringify({
        type: 'match',
        data: {
          path: { text: '/file.ts' },
          line_number: 1,
          submatches: [{ start: 0 }],
          lines: { text: 'content' },
        },
      });
      const results = parseRipgrepJsonOutput(output);
      expect(results).toHaveLength(1);
    });

    it('should skip invalid lines', () => {
      const results = parseRipgrepJsonOutput('invalid json');
      expect(results).toHaveLength(0);
    });
  });

  describe('extractFunctionBody', () => {
    it('should extract body', () => {
      const lines = ['function f() {', '  return 1;', '}'];
      const result = extractFunctionBody(lines, 0);
      expect(result).not.toBeNull();
      expect(result!.lines).toHaveLength(2);
    });

    it('should handle empty lines', () => {
      const lines = ['function f() {', '', '  return 1;', '}'];
      const result = extractFunctionBody(lines, 0);
      expect(result).not.toBeNull();
      expect(result!.lines).toHaveLength(3);
    });

    it('should return null if no brace', () => {
      const lines = ['function f()'];
      const result = extractFunctionBody(lines, 0);
      expect(result).toBeNull();
    });

    it('should handle multiple braces on same line', () => {
      // Tests the brace counting branch: line[j] === '{' braceCount++ and line[j] === '}' braceCount--
      const lines = ['function f() { if (true) { return 1; } }'];
      const result = extractFunctionBody(lines, 0);
      expect(result).not.toBeNull();
      // After the first '{', we count: '{ return 1; }' has +1 '{' and +2 '}' on same line
    });

    it('should handle nested braces across lines', () => {
      const lines = [
        'function outer() {',
        '  if (true) {',
        '    return { value: 1 };',
        '  }',
        '}',
      ];
      const result = extractFunctionBody(lines, 0);
      expect(result).not.toBeNull();
      expect(result!.lines.length).toBeGreaterThanOrEqual(3);
    });

    it('should handle inline brace on opening line', () => {
      // Cover line 1204-1206: counting braces within same line after opening brace
      const lines = ['function f() { const x = {}; return x; }'];
      const result = extractFunctionBody(lines, 0);
      expect(result).not.toBeNull();
    });

    it('should slice content before closing brace when brace is not at start', () => {
      // Cover line 1233-1234: lastBraceIndex > 0 case
      const lines = ['function f() {', '  return 1;', '  /* comment */ }'];
      const result = extractFunctionBody(lines, 0);
      expect(result).not.toBeNull();
      // The last line should be trimmed before the closing brace
    });
  });

  describe('inferSymbolKind', () => {
    it('should infer function', () => {
      expect(inferSymbolKind('function f() {}')).toBe('function');
    });

    it('should infer class', () => {
      expect(inferSymbolKind('class C {}')).toBe('class');
    });

    it('should infer interface', () => {
      expect(inferSymbolKind('interface I {}')).toBe('interface');
    });

    it('should infer const', () => {
      expect(inferSymbolKind('const c = 1;')).toBe('constant');
    });

    it('should infer variable', () => {
      expect(inferSymbolKind('let v = 1;')).toBe('variable');
    });

    it('should infer type', () => {
      expect(inferSymbolKind('type MyType = string;')).toBe('type');
    });

    it('should infer enum', () => {
      expect(inferSymbolKind('enum Status { Active, Inactive }')).toBe('enum');
    });

    it('should infer namespace', () => {
      expect(inferSymbolKind('namespace MyApp {}')).toBe('namespace');
    });

    it('should infer module', () => {
      expect(inferSymbolKind('module MyModule {}')).toBe('module');
    });

    it('should infer var as variable', () => {
      expect(inferSymbolKind('var x = 1;')).toBe('variable');
    });

    it('should infer const arrow function as function not constant', () => {
      expect(inferSymbolKind('const myFunc = () => {}')).toBe('function');
    });

    it('should infer const function as function not constant', () => {
      expect(inferSymbolKind('const myFunc = function() {}')).toBe('function');
    });

    it('should infer let arrow function as function not variable', () => {
      expect(inferSymbolKind('let myFunc = (x) => x')).toBe('function');
    });

    it('should not hang on ReDoS input with repeated = chars', () => {
      const start = Date.now();
      const malicious = 'const x =' + '='.repeat(1000);
      inferSymbolKind(malicious);
      expect(Date.now() - start).toBeLessThan(50);
    });

    it('should not hang on ReDoS input with repeated ( chars', () => {
      const start = Date.now();
      const malicious = 'const x =(' + '('.repeat(1000);
      inferSymbolKind(malicious);
      expect(Date.now() - start).toBeLessThan(50);
    });

    it('should infer var arrow function as function not variable', () => {
      expect(inferSymbolKind('var myFunc = (a, b) => a + b')).toBe('function');
    });

    it('should infer single-param arrow function as function', () => {
      expect(inferSymbolKind('const fn = x => x + 1')).toBe('function');
    });

    it('should infer async function expression as function', () => {
      expect(inferSymbolKind('const fn = async function() {}')).toBe(
        'function'
      );
    });
  });

  describe('isFunctionAssignment', () => {
    it('should return false when no = sign present', () => {
      expect(isFunctionAssignment('function foo() {}')).toBe(false);
      expect(isFunctionAssignment('class MyClass {}')).toBe(false);
      expect(isFunctionAssignment('const x')).toBe(false);
    });

    it('should detect function keyword after =', () => {
      expect(isFunctionAssignment('const fn = function() {}')).toBe(true);
      expect(isFunctionAssignment('let fn = function named() {}')).toBe(true);
      expect(isFunctionAssignment('var fn = async function() {}')).toBe(true);
    });

    it('should detect arrow function with parenthesized params', () => {
      expect(isFunctionAssignment('const fn = () => {}')).toBe(true);
      expect(isFunctionAssignment('const fn = (x) => x')).toBe(true);
      expect(isFunctionAssignment('const fn = (a, b) => a + b')).toBe(true);
      expect(isFunctionAssignment('let fn = async (x) => x')).toBe(true);
    });

    it('should detect single-param arrow function without parens', () => {
      expect(isFunctionAssignment('const fn = x => x + 1')).toBe(true);
      expect(isFunctionAssignment('let fn = item => item.id')).toBe(true);
    });

    it('should return false for non-function assignments', () => {
      expect(isFunctionAssignment('const x = 1;')).toBe(false);
      expect(isFunctionAssignment('let name = "hello";')).toBe(false);
      expect(isFunctionAssignment('var obj = {};')).toBe(false);
      expect(isFunctionAssignment('const arr = [1, 2, 3];')).toBe(false);
    });

    it('should not hang on ReDoS input with repeated = chars', () => {
      const start = Date.now();
      const malicious = 'const x =' + '='.repeat(10000);
      isFunctionAssignment(malicious);
      expect(Date.now() - start).toBeLessThan(50);
    });

    it('should not hang on ReDoS input with repeated ( chars', () => {
      const start = Date.now();
      const malicious = 'const x =(' + '('.repeat(10000);
      isFunctionAssignment(malicious);
      expect(Date.now() - start).toBeLessThan(50);
    });

    it('should not hang on ReDoS input with repeated =( pattern', () => {
      const start = Date.now();
      const malicious = 'const x =' + '=('.repeat(5000);
      isFunctionAssignment(malicious);
      expect(Date.now() - start).toBeLessThan(50);
    });

    it('should not hang on ReDoS input with repeated ) chars', () => {
      const start = Date.now();
      const malicious = 'const x =' + ')'.repeat(10000) + '=>';
      isFunctionAssignment(malicious);
      expect(Date.now() - start).toBeLessThan(50);
    });

    it('should handle empty string', () => {
      expect(isFunctionAssignment('')).toBe(false);
    });

    it('should handle = at end of line', () => {
      expect(isFunctionAssignment('const x =')).toBe(false);
    });

    it('should only check after the first = sign', () => {
      // "function" before = should not match
      expect(isFunctionAssignment('const x = 42')).toBe(false);
    });
  });

  describe('createRange', () => {
    it('should create range', () => {
      const range = createRange(0, 0, 5);
      expect(range).toEqual({
        start: { line: 0, character: 0 },
        end: { line: 0, character: 5 },
      });
    });

    it('should create range with non-zero character offset', () => {
      const range = createRange(10, 5, 15);
      expect(range).toEqual({
        start: { line: 10, character: 5 },
        end: { line: 10, character: 20 },
      });
    });
  });

  describe('createCallHierarchyItemFromSite - no selectionRange (stripped)', () => {
    beforeEach(() => {
      vi.clearAllMocks();
    });

    it('should not include selectionRange (internal field stripped)', async () => {
      const { createCallHierarchyItemFromSite } =
        await import('../../src/tools/lsp_call_hierarchy/callHierarchyHelpers.js');

      // Mock file with function definition
      const fileContent = `function processData(input) {\n  return input;\n}`;
      (fsPromises.readFile as Mock).mockResolvedValue(fileContent);

      const site = {
        filePath: '/workspace/file.ts',
        lineNumber: 1,
        lineContent: 'function processData(input) {',
        column: 9,
      };

      const result = await createCallHierarchyItemFromSite(site, 2);

      // selectionRange is no longer produced (C13 optimization)
      expect(result.selectionRange).toBeUndefined();
      // But range should still be present
      expect(result.range).toBeDefined();
      expect(typeof result.name).toBe('string');
    });
  });

  describe('createCallHierarchyItemFromSite - method matching', () => {
    beforeEach(() => {
      vi.clearAllMocks();
    });

    it('should detect method defined as myMethod(args) {', async () => {
      (toolHelpers.validateToolPath as Mock).mockReturnValue({
        isValid: true,
        sanitizedPath: '/workspace/file.ts',
      });

      // Content where a method is defined with myMethod(args): pattern
      const fileContent = `class Service {
  handleRequest(req: Request): void {
    this.myFunc();
  }
}`;
      (fsPromises.readFile as Mock).mockResolvedValue(fileContent);
      mockSymbolResolver.resolvePositionFromContent.mockReturnValue({
        position: { line: 2, character: 9 },
        foundAtLine: 3,
      });
      (managerModule.isLanguageServerAvailable as Mock).mockResolvedValue(
        false
      );
      (checkCommandAvailability as Mock).mockResolvedValue({
        available: true,
      });
      (safeExec as Mock).mockResolvedValue({
        success: true,
        stdout: JSON.stringify({
          type: 'match',
          data: {
            path: { text: '/workspace/caller.ts' },
            line_number: 3,
            lines: { text: '    this.myFunc();' },
            submatches: [{ start: 9, end: 15 }],
          },
        }),
      });

      const query = {
        uri: '/workspace/file.ts',
        symbolName: 'myFunc',
        lineHint: 3,
        direction: 'incoming',
        researchGoal: 'goal',
        reasoning: 'reason',
        mainResearchGoal: 'main',
      };

      const result = await toolHandler({ queries: [query] });
      const results = JSON.parse(result.content[0].text);

      expect(results[0].status).toBe('hasResults');
    });

    it('should detect async method pattern', async () => {
      (toolHelpers.validateToolPath as Mock).mockReturnValue({
        isValid: true,
        sanitizedPath: '/workspace/file.ts',
      });

      // Content with async method
      const fileContent = `class Service {
  async processData(data): Promise<void> {
    await myFunc();
  }
}`;
      (fsPromises.readFile as Mock).mockResolvedValue(fileContent);
      mockSymbolResolver.resolvePositionFromContent.mockReturnValue({
        position: { line: 2, character: 10 },
        foundAtLine: 3,
      });
      (managerModule.isLanguageServerAvailable as Mock).mockResolvedValue(
        false
      );
      (checkCommandAvailability as Mock).mockResolvedValue({
        available: true,
      });
      (safeExec as Mock).mockResolvedValue({
        success: true,
        stdout: JSON.stringify({
          type: 'match',
          data: {
            path: { text: '/workspace/caller.ts' },
            line_number: 3,
            lines: { text: '    await myFunc();' },
            submatches: [{ start: 10, end: 16 }],
          },
        }),
      });

      const query = {
        uri: '/workspace/file.ts',
        symbolName: 'myFunc',
        lineHint: 3,
        direction: 'incoming',
        researchGoal: 'goal',
        reasoning: 'reason',
        mainResearchGoal: 'main',
      };

      const result = await toolHandler({ queries: [query] });
      const results = JSON.parse(result.content[0].text);

      expect(results[0].status).toBe('hasResults');
    });

    it('should handle file read error gracefully in createCallHierarchyItemFromSite', async () => {
      (toolHelpers.validateToolPath as Mock).mockReturnValue({
        isValid: true,
        sanitizedPath: '/workspace/file.ts',
      });
      (fsPromises.readFile as Mock)
        .mockResolvedValueOnce('function myFunc() {}') // First call for main handler
        .mockRejectedValueOnce(new Error('File not found')); // Second call fails for call site

      mockSymbolResolver.resolvePositionFromContent.mockReturnValue({
        position: { line: 0, character: 0 },
        foundAtLine: 1,
      });
      (managerModule.isLanguageServerAvailable as Mock).mockResolvedValue(
        false
      );
      (checkCommandAvailability as Mock).mockResolvedValue({
        available: true,
      });
      (safeExec as Mock).mockResolvedValue({
        success: true,
        stdout: JSON.stringify({
          type: 'match',
          data: {
            path: { text: '/workspace/caller.ts' },
            line_number: 5,
            lines: { text: 'myFunc();' },
            submatches: [{ start: 0, end: 6 }],
          },
        }),
      });

      const query = {
        uri: '/workspace/file.ts',
        symbolName: 'myFunc',
        lineHint: 1,
        direction: 'incoming',
        researchGoal: 'goal',
        reasoning: 'reason',
        mainResearchGoal: 'main',
      };

      const result = await toolHandler({ queries: [query] });
      const results = JSON.parse(result.content[0].text);

      // Should still have results, but with default 'unknown' enclosing function
      expect(results[0].status).toBe('hasResults');
    });
  });
});
