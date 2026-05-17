/**
 * Tests for LSP Call Hierarchy tool - focuses on helper functions and registration
 * @module tools/lsp_call_hierarchy.test
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  parseRipgrepJsonOutput,
  extractFunctionBody,
  inferSymbolKind,
  createRange,
} from '../../src/tools/lsp_call_hierarchy/callHierarchy.js';

describe('LSP Call Hierarchy Tool', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.resetAllMocks();
  });

  describe('registerLSPCallHierarchyTool', () => {
    it('should register tool with correct name and schema', async () => {
      vi.resetModules();

      const { registerLSPCallHierarchyTool } =
        await import('../../src/tools/lsp_call_hierarchy/register.js');

      const mockServer = {
        registerTool: vi.fn().mockReturnValue(undefined),
      };

      registerLSPCallHierarchyTool(mockServer as any);

      const { STATIC_TOOL_NAMES } =
        await import('../../src/tools/toolNames.js');
      expect(mockServer.registerTool).toHaveBeenCalledWith(
        STATIC_TOOL_NAMES.LSP_CALL_HIERARCHY,
        expect.objectContaining({
          description: expect.any(String),
          inputSchema: expect.any(Object),
          annotations: expect.objectContaining({
            title: 'Call Hierarchy',
            readOnlyHint: true,
            destructiveHint: false,
            idempotentHint: true,
          }),
        }),
        expect.any(Function)
      );
    });

    it('should have correct annotations', async () => {
      vi.resetModules();

      const { registerLSPCallHierarchyTool } =
        await import('../../src/tools/lsp_call_hierarchy/register.js');

      const mockServer = {
        registerTool: vi.fn().mockReturnValue(undefined),
      };

      registerLSPCallHierarchyTool(mockServer as any);

      const callArgs = mockServer.registerTool.mock.calls[0]!;
      const toolConfig = callArgs[1];

      expect(toolConfig.annotations.openWorldHint).toBe(false);
    });

    it('should register handler function', async () => {
      vi.resetModules();

      const { registerLSPCallHierarchyTool } =
        await import('../../src/tools/lsp_call_hierarchy/register.js');

      const mockServer = {
        registerTool: vi.fn().mockReturnValue(undefined),
      };

      registerLSPCallHierarchyTool(mockServer as any);

      const handler = mockServer.registerTool.mock.calls[0]![2];
      expect(typeof handler).toBe('function');
    });
  });

  describe('Schema validation', () => {
    it('should export correct schema', async () => {
      vi.resetModules();

      const { BulkLSPCallHierarchySchema } =
        await import('@octocodeai/octocode-core');

      expect(BulkLSPCallHierarchySchema).toBeDefined();
      const parsed = BulkLSPCallHierarchySchema.safeParse({
        queries: [
          {
            id: 'call_hierarchy_schema',
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

    it('should have queries property in schema', async () => {
      vi.resetModules();

      const { BulkLSPCallHierarchySchema } =
        await import('@octocodeai/octocode-core');

      const parsed = BulkLSPCallHierarchySchema.safeParse({
        queries: [
          {
            id: 'call_hierarchy_queries',
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
  });

  describe('inferSymbolKind helper logic', () => {
    // Test the symbol kind inference logic
    const testCases = [
      { line: 'class MyClass {', expected: 'class' },
      { line: 'interface MyInterface {', expected: 'interface' },
      { line: 'type MyType = string;', expected: 'type' },
      { line: 'const MY_CONST = 1;', expected: 'constant' },
      { line: 'let myVar = 1;', expected: 'variable' },
      { line: 'var myVar = 1;', expected: 'variable' },
      { line: 'enum MyEnum {', expected: 'enum' },
      { line: 'namespace MyNS {', expected: 'namespace' },
      { line: 'module MyModule {', expected: 'module' },
      { line: 'function myFunc() {', expected: 'function' },
      { line: 'const myFunc = () => {', expected: 'function' },
    ];

    for (const { line, expected } of testCases) {
      it(`should infer "${expected}" from "${line}"`, () => {
        // Replicate inferSymbolKind logic
        let kind: string;
        if (/\bclass\b/.test(line)) kind = 'class';
        else if (/\binterface\b/.test(line)) kind = 'interface';
        else if (/\btype\b/.test(line)) kind = 'type';
        else if (
          /\bconst\b/.test(line) &&
          !/=.*(?:function|\([^)]*\)\s*=>)/.test(line)
        )
          kind = 'constant';
        else if (
          /\b(?:let|var)\b/.test(line) &&
          !/=.*(?:function|\([^)]*\)\s*=>)/.test(line)
        )
          kind = 'variable';
        else if (/\benum\b/.test(line)) kind = 'enum';
        else if (/\bnamespace\b/.test(line)) kind = 'namespace';
        else if (/\bmodule\b/.test(line)) kind = 'module';
        else kind = 'function';

        expect(kind).toBe(expected);
      });
    }
  });

  describe('extractFunctionBody helper logic', () => {
    // Test function body extraction logic
    it('should extract function body between braces', () => {
      const lines = ['function test() {', '  const x = 1;', '  return x;', '}'];

      let braceCount = 0;
      let foundStart = false;
      let bodyStartLine = 0;
      const bodyLines: string[] = [];

      for (let i = 0; i < lines.length; i++) {
        const line = lines[i]!;

        if (!foundStart) {
          const braceIndex = line.indexOf('{');
          if (braceIndex !== -1) {
            foundStart = true;
            bodyStartLine = i;
            braceCount = 1;
            bodyLines.push(line.slice(braceIndex + 1));
            continue;
          }
        }

        if (foundStart) {
          for (const char of line) {
            if (char === '{') braceCount++;
            if (char === '}') braceCount--;
          }

          if (braceCount > 0) {
            bodyLines.push(line);
          }
        }
      }

      expect(foundStart).toBe(true);
      expect(bodyStartLine).toBe(0);
      expect(bodyLines.length).toBe(3); // Empty after {, then 2 content lines
    });

    it('should handle nested braces', () => {
      const lines = [
        'function test() {',
        '  if (true) {',
        '    return 1;',
        '  }',
        '}',
      ];

      let braceCount = 0;
      let foundStart = false;
      const bodyLines: string[] = [];

      for (const line of lines) {
        if (!foundStart) {
          const braceIndex = line.indexOf('{');
          if (braceIndex !== -1) {
            foundStart = true;
            braceCount = 1;
            bodyLines.push(line.slice(braceIndex + 1));
            continue;
          }
        }

        if (foundStart) {
          for (const char of line) {
            if (char === '{') braceCount++;
            if (char === '}') braceCount--;
          }

          if (braceCount > 0) {
            bodyLines.push(line);
          }
        }
      }

      expect(bodyLines.length).toBe(4);
    });
  });

  describe('escapeRegex helper logic', () => {
    it('should escape regex special characters', () => {
      const escapeRegex = (str: string): string => {
        return str.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
      };

      expect(escapeRegex('test')).toBe('test');
      expect(escapeRegex('test()')).toBe('test\\(\\)');
      expect(escapeRegex('test.*')).toBe('test\\.\\*');
      expect(escapeRegex('$test')).toBe('\\$test');
      expect(escapeRegex('test[0]')).toBe('test\\[0\\]');
    });
  });

  describe('createRange helper logic', () => {
    it('should create valid LSP range', () => {
      const createRange = (
        line: number,
        character: number,
        length: number
      ) => ({
        start: { line, character },
        end: { line, character: character + length },
      });

      const range = createRange(5, 10, 20);

      expect(range.start.line).toBe(5);
      expect(range.start.character).toBe(10);
      expect(range.end.line).toBe(5);
      expect(range.end.character).toBe(30);
    });
  });

  describe('Pagination logic', () => {
    it('should calculate pagination correctly', () => {
      const items = Array(25).fill(null);
      const perPage = 10;
      const page = 2;

      const totalResults = items.length;
      const totalPages = Math.ceil(totalResults / perPage);
      const startIndex = (page - 1) * perPage;
      const paginatedItems = items.slice(startIndex, startIndex + perPage);

      expect(totalPages).toBe(3);
      expect(startIndex).toBe(10);
      expect(paginatedItems.length).toBe(10);
    });

    it('should handle last page with fewer items', () => {
      const items = Array(25).fill(null);
      const perPage = 10;
      const page = 3;

      const startIndex = (page - 1) * perPage;
      const paginatedItems = items.slice(startIndex, startIndex + perPage);

      expect(paginatedItems.length).toBe(5);
    });
  });

  describe('Call pattern detection', () => {
    // Test function call pattern matching
    it('should match function calls', () => {
      const line = '  const x = helperOne() + helperTwo();';
      const callPattern = /\b([a-zA-Z_$][a-zA-Z0-9_$]*)\s*\(/g;

      const calls: string[] = [];
      let match;
      while ((match = callPattern.exec(line)) !== null) {
        if (match[1]) calls.push(match[1]);
      }

      expect(calls).toContain('helperOne');
      expect(calls).toContain('helperTwo');
    });

    it('should exclude keywords from calls', () => {
      const excludePatterns = new Set([
        'if',
        'for',
        'while',
        'switch',
        'catch',
        'function',
        'return',
        'throw',
        'new',
        'typeof',
        'instanceof',
        'void',
        'delete',
        'await',
        'async',
      ]);

      const line = '  if (condition) { await something(); }';
      const callPattern = /\b([a-zA-Z_$][a-zA-Z0-9_$]*)\s*\(/g;

      const calls: string[] = [];
      let match;
      while ((match = callPattern.exec(line)) !== null) {
        if (match[1] && !excludePatterns.has(match[1])) {
          calls.push(match[1]);
        }
      }

      expect(calls).not.toContain('if');
      expect(calls).not.toContain('await');
      expect(calls).toContain('something');
    });
  });

  describe('Tool name constant', () => {
    it('should use correct tool name from constants', async () => {
      vi.resetModules();

      const { STATIC_TOOL_NAMES } =
        await import('../../src/tools/toolNames.js');

      expect(STATIC_TOOL_NAMES.LSP_CALL_HIERARCHY).toBeDefined();
      expect(typeof STATIC_TOOL_NAMES.LSP_CALL_HIERARCHY).toBe('string');
    });
  });

  describe('Description export', () => {
    it('should export tool description', async () => {
      // Don't reset modules - use the initialized metadata from setup.ts
      const { LSP_CALL_HIERARCHY_DESCRIPTION } =
        await import('@octocodeai/octocode-core');

      expect(LSP_CALL_HIERARCHY_DESCRIPTION).toBeDefined();
      expect(typeof LSP_CALL_HIERARCHY_DESCRIPTION).toBe('string');
      // Description may be empty if tool not in remote metadata (local-only tool)
    });
  });

  describe('parseRipgrepJsonOutput', () => {
    it('should parse valid ripgrep JSON output', () => {
      const output = `{"type":"match","data":{"path":{"text":"/test.ts"},"line_number":5,"submatches":[{"start":2}],"lines":{"text":"  myFunc()\\n"}}}`;

      const results = parseRipgrepJsonOutput(output);

      expect(results.length).toBe(1);
      expect(results[0]!.filePath).toBe('/test.ts');
      expect(results[0]!.lineNumber).toBe(5);
      expect(results[0]!.column).toBe(2);
    });

    it('should skip invalid JSON lines', () => {
      const output = `invalid json
{"type":"match","data":{"path":{"text":"/test.ts"},"line_number":5,"submatches":[{"start":0}],"lines":{"text":"line"}}}`;

      const results = parseRipgrepJsonOutput(output);
      expect(results.length).toBe(1);
    });

    it('should skip non-match entries', () => {
      const output = `{"type":"begin"}
{"type":"match","data":{"path":{"text":"/test.ts"},"line_number":5,"submatches":[{"start":0}],"lines":{"text":"line"}}}
{"type":"end"}`;

      const results = parseRipgrepJsonOutput(output);
      expect(results.length).toBe(1);
    });

    it('should handle empty output', () => {
      const results = parseRipgrepJsonOutput('');
      expect(results.length).toBe(0);
    });

    it('should handle multiple matches', () => {
      const output = `{"type":"match","data":{"path":{"text":"/a.ts"},"line_number":1,"submatches":[{"start":0}],"lines":{"text":"a"}}}
{"type":"match","data":{"path":{"text":"/b.ts"},"line_number":2,"submatches":[{"start":0}],"lines":{"text":"b"}}}
{"type":"match","data":{"path":{"text":"/c.ts"},"line_number":3,"submatches":[{"start":0}],"lines":{"text":"c"}}}`;

      const results = parseRipgrepJsonOutput(output);
      expect(results.length).toBe(3);
    });
  });

  describe('extractFunctionBody comprehensive tests', () => {
    it('should return null for empty lines', () => {
      const result = extractFunctionBody([], 0);
      expect(result).toBeNull();
    });

    it('should handle function with parameters on multiple lines', () => {
      const lines = [
        'function test(',
        '  param1: string,',
        '  param2: number',
        ') {',
        '  return param1;',
        '}',
      ];

      const result = extractFunctionBody(lines, 0);
      expect(result).not.toBeNull();
    });

    it('should handle class methods', () => {
      const lines = [
        'class MyClass {',
        '  myMethod() {',
        '    return 1;',
        '  }',
        '}',
      ];

      const result = extractFunctionBody(lines, 0);
      expect(result).not.toBeNull();
    });
  });

  describe('createRange edge cases', () => {
    it('should handle large line numbers', () => {
      const range = createRange(999999, 50, 10);
      expect(range.start.line).toBe(999999);
      expect(range.end.line).toBe(999999);
    });

    it('should handle zero length', () => {
      const range = createRange(5, 10, 0);
      expect(range.end.character).toBe(10);
    });
  });

  describe('inferSymbolKind comprehensive', () => {
    it('should handle export statements', () => {
      expect(inferSymbolKind('export function test() {')).toBe('function');
      expect(inferSymbolKind('export class MyClass {')).toBe('class');
      expect(inferSymbolKind('export const x = 1;')).toBe('constant');
    });

    it('should handle async arrow functions', () => {
      expect(inferSymbolKind('const fn = async () => {')).toBe('function');
    });

    it('should handle public/private methods', () => {
      expect(inferSymbolKind('public myMethod() {')).toBe('function');
      expect(inferSymbolKind('private helper() {')).toBe('function');
    });
  });
});
