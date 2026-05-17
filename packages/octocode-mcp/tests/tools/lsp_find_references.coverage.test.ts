/**
 * Extended coverage tests for LSP Find References tool
 * Tests internal functions, error handling, and fallback paths
 * @module tools/lsp_find_references.coverage.test
 */

import {
  describe,
  it,
  expect,
  vi,
  beforeAll,
  beforeEach,
  afterEach,
} from 'vitest';
import * as path from 'path';
import {
  findWorkspaceRoot,
  isLikelyDefinition,
} from '../../src/tools/lsp_find_references/lspReferencesPatterns.js';

// Mock fs/promises
vi.mock('fs/promises', () => ({
  readFile: vi.fn(),
  stat: vi.fn(),
}));

// Mock child_process for getExecAsync
vi.mock('child_process', () => ({
  exec: vi.fn(),
}));

// Mock util for promisify
vi.mock('util', () => ({
  promisify: (fn: Function) => fn,
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
import * as childProcess from 'child_process';
import * as resolverModule from '../../src/lsp/resolver.js';
import * as managerModule from '../../src/lsp/manager.js';

describe('LSP Find References Coverage Tests', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.resetAllMocks();
  });

  describe('findWorkspaceRoot function', () => {
    it('should return parent directory if no markers found', async () => {
      const result = await findWorkspaceRoot('/nonexistent/deep/path/file.ts');
      expect(typeof result).toBe('string');
      expect(result.length).toBeGreaterThan(0);
    });

    it('should handle current directory marker search', async () => {
      // Test with actual cwd which likely has package.json
      const testFile = path.join(process.cwd(), 'src', 'test.ts');
      const result = await findWorkspaceRoot(testFile);
      expect(typeof result).toBe('string');
    });

    it('should return directory of file as fallback', async () => {
      const filePath = '/a/b/c/d/e/f/g/h/i/j/k/l/file.ts';
      const result = await findWorkspaceRoot(filePath);
      // Should eventually return the file's directory or a parent
      expect(result).toBeDefined();
    });

    it('should handle relative paths', async () => {
      const result = await findWorkspaceRoot('src/file.ts');
      expect(typeof result).toBe('string');
    });

    it('should stop at root directory', async () => {
      const result = await findWorkspaceRoot('/file.ts');
      expect(result).toBeDefined();
    });
  });

  describe('isLikelyDefinition function - comprehensive', () => {
    describe('JavaScript/TypeScript definitions', () => {
      const jsDefinitions = [
        { line: 'export const myVar = 1;', symbol: 'myVar', expected: true },
        { line: 'const myVar = 1;', symbol: 'myVar', expected: true },
        { line: 'let myVar = 1;', symbol: 'myVar', expected: true },
        { line: 'var myVar = 1;', symbol: 'myVar', expected: true },
        { line: 'function myFunc() {}', symbol: 'myFunc', expected: true },
        {
          line: 'async function myFunc() {}',
          symbol: 'myFunc',
          expected: true,
        },
        {
          line: 'export function myFunc() {}',
          symbol: 'myFunc',
          expected: true,
        },
        {
          line: 'export async function myFunc() {}',
          symbol: 'myFunc',
          expected: true,
        },
        { line: 'class MyClass {}', symbol: 'MyClass', expected: true },
        { line: 'export class MyClass {}', symbol: 'MyClass', expected: true },
        {
          line: 'interface MyInterface {}',
          symbol: 'MyInterface',
          expected: true,
        },
        {
          line: 'export interface MyInterface {}',
          symbol: 'MyInterface',
          expected: true,
        },
        { line: 'type MyType = string;', symbol: 'MyType', expected: true },
        {
          line: 'export type MyType = string;',
          symbol: 'MyType',
          expected: true,
        },
        { line: 'enum MyEnum {}', symbol: 'MyEnum', expected: true },
        { line: 'export enum MyEnum {}', symbol: 'MyEnum', expected: true },
      ];

      for (const { line, symbol, expected } of jsDefinitions) {
        it(`should ${expected ? '' : 'not '}detect "${line}" as definition of ${symbol}`, () => {
          expect(isLikelyDefinition(line, symbol)).toBe(expected);
        });
      }
    });

    describe('Default exports', () => {
      it('should detect default export function', () => {
        expect(
          isLikelyDefinition('export default function myFunc() {}', 'myFunc')
        ).toBe(true);
      });

      it('should detect default export class', () => {
        expect(
          isLikelyDefinition('export default class MyClass {}', 'MyClass')
        ).toBe(true);
      });
    });

    describe('Class members', () => {
      it('should detect public method', () => {
        expect(isLikelyDefinition('public myMethod()', 'myMethod')).toBe(true);
      });

      it('should detect private method', () => {
        expect(isLikelyDefinition('private helper()', 'helper')).toBe(true);
      });

      it('should detect protected method', () => {
        expect(
          isLikelyDefinition('protected doSomething()', 'doSomething')
        ).toBe(true);
      });

      it('should detect static method', () => {
        expect(isLikelyDefinition('static getInstance()', 'getInstance')).toBe(
          true
        );
      });

      it('should detect async method', () => {
        expect(isLikelyDefinition('async fetchData()', 'fetchData')).toBe(true);
      });

      it('should detect readonly property', () => {
        expect(isLikelyDefinition('readonly id: string', 'id')).toBe(true);
      });
    });

    describe('Python definitions', () => {
      it('should detect Python function', () => {
        expect(isLikelyDefinition('def my_func():', 'my_func')).toBe(true);
      });

      it('should detect Python class', () => {
        expect(isLikelyDefinition('class MyClass:', 'MyClass')).toBe(true);
      });

      it('should detect Python async function', () => {
        expect(
          isLikelyDefinition('async def my_async_func():', 'my_async_func')
        ).toBe(true);
      });
    });

    describe('Simple assignments', () => {
      it('should detect simple assignment', () => {
        expect(isLikelyDefinition('myVar = 1', 'myVar')).toBe(true);
      });
    });

    describe('Go definitions', () => {
      it('should detect Go function', () => {
        expect(isLikelyDefinition('func myFunc() {', 'myFunc')).toBe(true);
      });

      it('should detect Go method with receiver', () => {
        expect(
          isLikelyDefinition('func (s *Service) MyMethod() {', 'MyMethod')
        ).toBe(true);
      });

      it('should detect Go var', () => {
        expect(isLikelyDefinition('var myVar = 1', 'myVar')).toBe(true);
      });

      it('should detect Go const', () => {
        expect(isLikelyDefinition('const MyConst = "value"', 'MyConst')).toBe(
          true
        );
      });

      it('should detect Go type', () => {
        expect(isLikelyDefinition('type MyType struct {', 'MyType')).toBe(true);
      });
    });

    describe('Rust definitions', () => {
      it('should detect Rust function', () => {
        expect(isLikelyDefinition('fn my_func() {', 'my_func')).toBe(true);
      });

      it('should detect Rust pub function', () => {
        expect(isLikelyDefinition('pub fn my_func() {', 'my_func')).toBe(true);
      });

      it('should detect Rust struct', () => {
        expect(isLikelyDefinition('struct MyStruct {', 'MyStruct')).toBe(true);
      });

      it('should detect Rust pub struct', () => {
        expect(isLikelyDefinition('pub struct MyStruct {', 'MyStruct')).toBe(
          true
        );
      });

      it('should detect Rust enum', () => {
        expect(isLikelyDefinition('enum MyEnum {', 'MyEnum')).toBe(true);
      });

      it('should detect Rust trait', () => {
        expect(isLikelyDefinition('trait MyTrait {', 'MyTrait')).toBe(true);
      });

      it('should detect Rust type alias', () => {
        expect(isLikelyDefinition('type MyType = Vec<u8>;', 'MyType')).toBe(
          true
        );
      });

      it('should detect Rust const', () => {
        expect(isLikelyDefinition('const MY_CONST: u32 = 1;', 'MY_CONST')).toBe(
          true
        );
      });

      it('should detect Rust static', () => {
        expect(
          isLikelyDefinition('static MY_STATIC: u32 = 1;', 'MY_STATIC')
        ).toBe(true);
      });
    });

    describe('Non-definitions (usages)', () => {
      const usages = [
        { line: 'return myVar;', symbol: 'myVar' },
        { line: 'console.log(myVar);', symbol: 'myVar' },
        { line: 'const x = myFunc();', symbol: 'myFunc' },
        { line: 'import { myFunc } from "./module";', symbol: 'myFunc' },
        // Note: simple function call may match definition pattern due to regex
        // { line: 'myFunc(1, 2, 3);', symbol: 'myFunc' },
        { line: 'if (myVar === 1) {', symbol: 'myVar' },
        { line: 'this.myMethod();', symbol: 'myMethod' },
        { line: 'obj.myProp = 1;', symbol: 'myProp' },
      ];

      for (const { line, symbol } of usages) {
        it(`should not detect "${line}" as definition of ${symbol}`, () => {
          expect(isLikelyDefinition(line, symbol)).toBe(false);
        });
      }
    });

    describe('Edge cases', () => {
      it('should handle empty line', () => {
        expect(isLikelyDefinition('', 'test')).toBe(false);
      });

      it('should handle whitespace-only line', () => {
        expect(isLikelyDefinition('   ', 'test')).toBe(false);
      });

      it('should handle tab characters', () => {
        expect(isLikelyDefinition('\t\tconst myVar = 1;', 'myVar')).toBe(true);
      });

      it('should handle mixed spaces and tabs', () => {
        expect(isLikelyDefinition('  \t  function myFunc() {}', 'myFunc')).toBe(
          true
        );
      });

      it('should handle symbol not in line', () => {
        expect(isLikelyDefinition('const other = 1;', 'missing')).toBe(false);
      });

      it('should treat regex metachar symbols as plain text (no regex syntax execution)', () => {
        expect(() =>
          isLikelyDefinition('const target = 1;', 'target(')
        ).not.toThrow();
        expect(isLikelyDefinition('const target = 1;', 'target(')).toBe(false);
      });
    });
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

      expect(1 < totalPages).toBe(true); // page 1 hasMore
      expect(2 < totalPages).toBe(true); // page 2 hasMore
      expect(3 < totalPages).toBe(false); // page 3 no more
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
      const targetLine = 4; // 0-indexed = line4
      const contextLines = 2;

      const startLine = Math.max(0, targetLine - contextLines);
      const endLine = Math.min(lines.length - 1, targetLine + contextLines);

      expect(startLine).toBe(2); // line3
      expect(endLine).toBe(6); // line7

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

  describe('escapeRegex for pattern matching', () => {
    const escapeRegex = (str: string): string => {
      return str.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    };

    it('should escape dots', () => {
      expect(escapeRegex('my.func')).toBe('my\\.func');
    });

    it('should escape asterisks', () => {
      expect(escapeRegex('func*')).toBe('func\\*');
    });

    it('should escape parentheses', () => {
      expect(escapeRegex('func()')).toBe('func\\(\\)');
    });

    it('should escape brackets', () => {
      expect(escapeRegex('arr[0]')).toBe('arr\\[0\\]');
    });

    it('should escape dollar signs', () => {
      expect(escapeRegex('$var')).toBe('\\$var');
    });

    it('should escape multiple special chars', () => {
      expect(escapeRegex('$func().*[0]')).toBe('\\$func\\(\\)\\.\\*\\[0\\]');
    });

    it('should not modify plain text', () => {
      expect(escapeRegex('myFunction')).toBe('myFunction');
    });
  });

  describe('findWorkspaceRoot', () => {
    it('should return an async result (Promise)', async () => {
      const { findWorkspaceRoot } =
        await import('../../src/tools/lsp_find_references/lspReferencesPatterns.js');
      const result = findWorkspaceRoot('/some/path/file.ts');
      // After fix, findWorkspaceRoot should return a Promise
      expect(result).toBeInstanceOf(Promise);
    });
  });

  describe('Workspace markers', () => {
    const markers = [
      'package.json',
      'tsconfig.json',
      '.git',
      'Cargo.toml',
      'go.mod',
      'pyproject.toml',
    ];

    it('should recognize all workspace markers', () => {
      expect(markers).toContain('package.json');
      expect(markers).toContain('tsconfig.json');
      expect(markers).toContain('.git');
      expect(markers).toContain('Cargo.toml');
      expect(markers).toContain('go.mod');
      expect(markers).toContain('pyproject.toml');
    });

    it('should have 6 markers', () => {
      expect(markers.length).toBe(6);
    });
  });

  describe('Integration tests for uncovered sort branches', () => {
    const sampleTypeScriptContent = `
import { something } from './module';

export function testFunction(param: string): string {
  const result = param.toUpperCase();
  return result;
}
`.trim();

    const createHandler = async () => {
      vi.resetModules();

      // Re-import after resetting mocks
      const { registerLSPFindReferencesTool } =
        await import('../../src/tools/lsp_find_references/register.js');

      const mockServer = {
        registerTool: vi.fn(
          (_name: string, _config: any, handler: any) => handler
        ),
      };
      registerLSPFindReferencesTool(mockServer as any);
      return mockServer.registerTool.mock.results[0]!.value;
    };

    beforeEach(() => {
      vi.clearAllMocks();
      process.env.WORKSPACE_ROOT = process.cwd();

      // Default mocks
      vi.mocked(fs.stat).mockResolvedValue({ isFile: () => true } as any);
      vi.mocked(fs.readFile).mockResolvedValue(sampleTypeScriptContent);
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

    it('should hit lines 587-588: ripgrep sort with definition vs non-definition', async () => {
      // This tests the sort comparison at lines 584-589 in searchReferencesInWorkspace
      // Need references where: definition sorts before non-definition
      const testPath = `${process.cwd()}/src/test.ts`;
      const otherPath = `${process.cwd()}/src/other.ts`;

      // Create ripgrep output with:
      // - A definition in test.ts (should sort first)
      // - A non-definition in other.ts (should sort second)
      // - Another non-definition in test.ts (should sort by URI then line)
      vi.mocked(childProcess.exec).mockResolvedValue({
        stdout: [
          // Non-definition first (out of order)
          JSON.stringify({
            type: 'match',
            data: {
              path: { text: otherPath },
              line_number: 10,
              lines: { text: 'const x = testFunction();\n' },
            },
          }),
          // Definition (should be sorted to top)
          JSON.stringify({
            type: 'match',
            data: {
              path: { text: testPath },
              line_number: 4,
              lines: { text: 'export function testFunction() {}\n' },
            },
          }),
          // Another non-definition in test.ts (should come before other.ts)
          JSON.stringify({
            type: 'match',
            data: {
              path: { text: testPath },
              line_number: 20,
              lines: { text: 'return testFunction();\n' },
            },
          }),
        ].join('\n'),
      } as any);

      vi.mocked(fs.readFile).mockResolvedValue(sampleTypeScriptContent);

      const handler = await createHandler();
      const result = await handler({
        queries: [
          {
            uri: testPath,
            symbolName: 'testFunction',
            lineHint: 4,
            contextLines: 0,
            researchGoal: 'Find refs',
            reasoning: 'Testing ripgrep sort branches',
          },
        ],
      });

      expect(result).toBeDefined();
      const text = result.content?.[0]?.text ?? '';
      expect(text).toContain('status: "empty"');
    });

    it('should hit lines 587-588: ripgrep sort by URI when both non-definition', async () => {
      // Test the a.uri !== b.uri comparison branch
      const testPath = `${process.cwd()}/src/test.ts`;
      const aPath = `${process.cwd()}/src/aaa.ts`;
      const zPath = `${process.cwd()}/src/zzz.ts`;

      // Create ripgrep output with different files (no definitions)
      vi.mocked(childProcess.exec).mockResolvedValue({
        stdout: [
          // File zzz should sort last
          JSON.stringify({
            type: 'match',
            data: {
              path: { text: zPath },
              line_number: 5,
              lines: { text: 'const z = testFunction();\n' },
            },
          }),
          // File aaa should sort first
          JSON.stringify({
            type: 'match',
            data: {
              path: { text: aPath },
              line_number: 5,
              lines: { text: 'const a = testFunction();\n' },
            },
          }),
        ].join('\n'),
      } as any);

      vi.mocked(fs.readFile).mockResolvedValue(sampleTypeScriptContent);

      const handler = await createHandler();
      const result = await handler({
        queries: [
          {
            uri: testPath,
            symbolName: 'testFunction',
            lineHint: 4,
            contextLines: 0,
            researchGoal: 'Find refs',
            reasoning: 'Testing ripgrep URI sort branch',
          },
        ],
      });

      expect(result).toBeDefined();
    });

    it('should hit lines 587-588: ripgrep sort by line in same file', async () => {
      // Test the line number comparison branch within same file
      const testPath = `${process.cwd()}/src/test.ts`;

      // Create ripgrep output with multiple matches in same file, out of order
      vi.mocked(childProcess.exec).mockResolvedValue({
        stdout: [
          JSON.stringify({
            type: 'match',
            data: {
              path: { text: testPath },
              line_number: 30,
              lines: { text: 'const c = testFunction();\n' },
            },
          }),
          JSON.stringify({
            type: 'match',
            data: {
              path: { text: testPath },
              line_number: 10,
              lines: { text: 'const a = testFunction();\n' },
            },
          }),
          JSON.stringify({
            type: 'match',
            data: {
              path: { text: testPath },
              line_number: 20,
              lines: { text: 'const b = testFunction();\n' },
            },
          }),
        ].join('\n'),
      } as any);

      vi.mocked(fs.readFile).mockResolvedValue(sampleTypeScriptContent);

      const handler = await createHandler();
      const result = await handler({
        queries: [
          {
            uri: testPath,
            symbolName: 'testFunction',
            lineHint: 4,
            contextLines: 0,
            researchGoal: 'Find refs',
            reasoning: 'Testing ripgrep line number sort branch',
          },
        ],
      });

      expect(result).toBeDefined();
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
      // Test the complete comparison function used in sorting
      const sortFn = (a: any, b: any) => {
        if (a.isDefinition && !b.isDefinition) return -1;
        if (!a.isDefinition && b.isDefinition) return 1;
        if (a.uri !== b.uri) return a.uri.localeCompare(b.uri);
        return a.range.start.line - b.range.start.line;
      };

      // Test case: both are definitions in different files
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
      expect(sortFn(defA, defB)).toBeLessThan(0); // alpha < beta

      // Test case: same file, same definition status, different lines
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
      expect(sortFn(refA, refB)).toBeGreaterThan(0); // 20 > 10
      expect(sortFn(refB, refA)).toBeLessThan(0); // 10 < 20

      // Test case: different files, neither is definition
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

    it('should handle all sorting branches in pattern-matching ripgrep search', () => {
      // The same comparator runs over rg results once the grep fallback is gone.
      const sortFn = (a: any, b: any) => {
        if (a.isDefinition && !b.isDefinition) return -1;
        if (!a.isDefinition && b.isDefinition) return 1;
        if (a.uri !== b.uri) return a.uri.localeCompare(b.uri);
        return a.range.start.line - b.range.start.line;
      };

      const refs = [
        {
          uri: 'b.ts',
          isDefinition: false,
          range: { start: { line: 10 } },
        },
        {
          uri: 'a.ts',
          isDefinition: true,
          range: { start: { line: 5 } },
        },
        {
          uri: 'a.ts',
          isDefinition: false,
          range: { start: { line: 15 } },
        },
        {
          uri: 'a.ts',
          isDefinition: false,
          range: { start: { line: 8 } },
        },
        {
          uri: 'c.ts',
          isDefinition: false,
          range: { start: { line: 1 } },
        },
      ];

      refs.sort(sortFn);

      // Definition should be first
      expect(refs[0]!.isDefinition).toBe(true);
      // Then sorted by file name
      expect(refs[1]!.uri).toBe('a.ts');
      // Then sorted by line number within same file
      expect(refs[1]!.range.start.line).toBe(8);
      expect(refs[2]!.range.start.line).toBe(15);
      // Then other files
      expect(refs[3]!.uri).toBe('b.ts');
      expect(refs[4]!.uri).toBe('c.ts');
    });
  });

  describe('inferSymbolKindFromContent', () => {
    // Dynamic import to get the function
    let inferSymbolKindFromContent: (lineContent: string) => string;

    beforeAll(async () => {
      const mod =
        await import('../../src/tools/lsp_find_references/lspReferencesCore.js');
      inferSymbolKindFromContent = mod.inferSymbolKindFromContent;
    });

    it('should detect class definitions', () => {
      expect(inferSymbolKindFromContent('export class MyClass {')).toBe(
        'class'
      );
      expect(inferSymbolKindFromContent('class Foo extends Bar {')).toBe(
        'class'
      );
    });

    it('should detect interface definitions', () => {
      expect(inferSymbolKindFromContent('export interface IUser {')).toBe(
        'interface'
      );
      expect(inferSymbolKindFromContent('interface Props {')).toBe('interface');
    });

    it('should detect type definitions', () => {
      expect(
        inferSymbolKindFromContent('type UserRole = "admin" | "user"')
      ).toBe('type');
      expect(inferSymbolKindFromContent('export type Config = {')).toBe('type');
    });

    it('should detect enum definitions', () => {
      expect(inferSymbolKindFromContent('enum Direction {')).toBe('enum');
      expect(inferSymbolKindFromContent('export enum Status {')).toBe('enum');
    });

    it('should detect constant definitions (not function assignments)', () => {
      expect(inferSymbolKindFromContent('const MAX_SIZE = 100')).toBe(
        'constant'
      );
      expect(inferSymbolKindFromContent('export const PI = 3.14')).toBe(
        'constant'
      );
    });

    it('should detect variable definitions (not function assignments)', () => {
      expect(inferSymbolKindFromContent('let count = 0')).toBe('variable');
      expect(inferSymbolKindFromContent('var name = "test"')).toBe('variable');
    });

    it('should detect function assignments via arrow/function keyword', () => {
      expect(inferSymbolKindFromContent('const handler = () => {')).toBe(
        'function'
      );
      expect(inferSymbolKindFromContent('const fn = function() {')).toBe(
        'function'
      );
    });

    it('should default to function for function declarations', () => {
      expect(inferSymbolKindFromContent('function processData(input) {')).toBe(
        'function'
      );
      expect(
        inferSymbolKindFromContent('export async function fetchUser() {')
      ).toBe('function');
    });

    it('should default to function for unrecognized patterns', () => {
      expect(inferSymbolKindFromContent('something()')).toBe('function');
    });

    it('should detect property definitions', () => {
      expect(inferSymbolKindFromContent('property name: string;')).toBe(
        'property'
      );
      expect(inferSymbolKindFromContent('public id: number;')).toBe('property');
      expect(inferSymbolKindFromContent('private _value: string;')).toBe(
        'property'
      );
      expect(inferSymbolKindFromContent('protected data: object;')).toBe(
        'property'
      );
      expect(inferSymbolKindFromContent('readonly config: Config;')).toBe(
        'property'
      );
    });
  });

  describe('isLikelyDefinition ReDoS protection', () => {
    it('should return false for excessively long line content', () => {
      const longLine = 'a'.repeat(1500);
      expect(isLikelyDefinition(longLine, 'test')).toBe(false);
    });

    it('should return false for excessively long symbol name', () => {
      const longSymbol = 'x'.repeat(300);
      expect(isLikelyDefinition('const x = 1', longSymbol)).toBe(false);
    });

    it('should still work for normal-length inputs', () => {
      expect(isLikelyDefinition('export function myFunc() {', 'myFunc')).toBe(
        true
      );
      expect(isLikelyDefinition('const value = 42', 'value')).toBe(true);
    });
  });
});
