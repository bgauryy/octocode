import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

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

import * as fs from 'fs/promises';
import * as resolverModule from '../../src/lsp/resolver.js';
import * as managerModule from '../../src/lsp/manager.js';

import { registerLSPGotoDefinitionTool } from '../../src/tools/lsp_goto_definition/lsp_goto_definition.js';
import { isImportOrReExport } from '../../src/tools/lsp_goto_definition/execution.js';

describe('LSP Goto Definition Implementation Tests', () => {
  const sampleTypeScriptContent = `
import { helper } from './utils';

export function mainFunction(param: string): string {
  const result = helper(param);
  return result.toUpperCase();
}

export interface Config {
  name: string;
  value: number;
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

  const createHandler = () => {
    const mockServer = {
      registerTool: vi.fn((_name, _config, handler) => handler),
    };
    registerLSPGotoDefinitionTool(mockServer as any);
    return mockServer.registerTool.mock.results[0]!.value;
  };

  describe('Tool Registration', () => {
    it('should register the tool with correct name', () => {
      const mockServer = {
        registerTool: vi.fn(),
      };

      registerLSPGotoDefinitionTool(mockServer as any);

      expect(mockServer.registerTool).toHaveBeenCalledWith(
        'lspGotoDefinition',
        expect.any(Object),
        expect.any(Function)
      );
    });
  });

  describe('Error Handling', () => {
    it('returns structured error when definition capability is unsupported', async () => {
      vi.mocked(managerModule.isLanguageServerAvailable).mockResolvedValue(
        true
      );
      const mockClient = {
        hasCapability: vi.fn().mockReturnValue(false),
        gotoDefinition: vi.fn(),
      };
      vi.mocked(managerModule.acquirePooledClient).mockResolvedValue(
        mockClient as any
      );

      const handler = createHandler();
      const result = await handler({
        queries: [
          {
            uri: `${process.cwd()}/src/file.ts`,
            symbolName: 'helper',
            lineHint: 4,
            researchGoal: 'Find def',
            reasoning: 'Testing capability gating',
          },
        ],
      });

      const text = result.content?.[0]?.text ?? '';
      expect(text).toContain('status: "error"');
      expect(text).toContain('errorCode: "LSP_CAPABILITY_UNSUPPORTED"');
      expect(text).not.toContain('lspMode: "semantic"');
      expect(mockClient.gotoDefinition).not.toHaveBeenCalled();
    });

    it('should handle file read errors', async () => {
      vi.mocked(fs.readFile).mockRejectedValue(new Error('Permission denied'));

      const handler = createHandler();
      const result = await handler({
        queries: [
          {
            uri: '/workspace/protected.ts',
            symbolName: 'test',
            lineHint: 1,
            researchGoal: 'Find def',
            reasoning: 'Testing',
          },
        ],
      });

      expect(result).toBeDefined();
      expect(result.content?.length).toBeGreaterThan(0);
    });
  });

  describe('Symbol resolution errors', () => {
    it('should return empty result when symbol cannot be resolved', async () => {
      process.env.WORKSPACE_ROOT = process.cwd();
      const testPath = `${process.cwd()}/src/test.ts`;

      vi.mocked(fs.readFile).mockResolvedValue('const somethingElse = 1;');

      const symbolError = new (resolverModule as any).SymbolResolutionError(
        'Symbol not found',
        2
      );
      vi.mocked(resolverModule.SymbolResolver).mockImplementation(function () {
        return {
          resolvePositionFromContent: vi.fn(() => {
            throw symbolError;
          }),
          extractContext: vi.fn(),
        };
      });

      const handler = createHandler();
      const result = await handler({
        queries: [
          {
            uri: testPath,
            symbolName: 'missingSymbol',
            lineHint: 10,
            researchGoal: 'Find definition',
            reasoning: 'Testing symbol resolution error',
          },
        ],
      });

      const text = result.content?.[0]?.text ?? '';
      expect(text).toContain('status: "empty"');
      expect(text).toContain('errorType: "symbol_not_found"');
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
            researchGoal: 'Find def',
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
            researchGoal: 'Find def',
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
            researchGoal: 'Find A',
            reasoning: 'Test A',
          },
          {
            uri: '/workspace/src/b.ts',
            symbolName: 'funcB',
            lineHint: 20,
            researchGoal: 'Find B',
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
            researchGoal: 'Find definition',
            reasoning: 'Testing',
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
            contextLines: 10,
            researchGoal: 'Find def',
            reasoning: 'Testing context',
          },
        ],
      });

      expect(result).toBeDefined();
    });

    it('should handle orderHint parameter', async () => {
      const handler = createHandler();
      const result = await handler({
        queries: [
          {
            uri: '/workspace/test.ts',
            symbolName: 'test',
            lineHint: 1,
            orderHint: 2,
            researchGoal: 'Find def',
            reasoning: 'Testing',
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
            researchGoal: 'Find def',
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
        gotoDefinition: vi.fn().mockResolvedValue([]),
      } as any);

      const handler = createHandler();
      const result = await handler({
        queries: [
          {
            uri: '/workspace/test.ts',
            symbolName: 'test',
            lineHint: 1,
            researchGoal: 'Find def',
            reasoning: 'Testing',
          },
        ],
      });

      expect(result).toBeDefined();
    });

    it('should return fallback result when LSP is unavailable', async () => {
      vi.mocked(managerModule.isLanguageServerAvailable).mockResolvedValue(
        false
      );

      const handler = createHandler();
      const result = await handler({
        queries: [
          {
            uri: '/workspace/test.ts',
            symbolName: 'test',
            lineHint: 1,
            researchGoal: 'Find def',
            reasoning: 'Testing',
          },
        ],
      });

      expect(result).toBeDefined();
      expect(result.content?.length).toBeGreaterThan(0);
    });
  });

  describe('LSP enhanced location formatting', () => {
    it('should enhance locations with numbered context when LSP returns definitions', async () => {
      process.env.WORKSPACE_ROOT = process.cwd();
      const testPath = `${process.cwd()}/src/test.ts`;
      const defsPath = `${process.cwd()}/src/defs.ts`;

      vi.mocked(managerModule.isLanguageServerAvailable).mockResolvedValue(
        true
      );
      vi.mocked(managerModule.acquirePooledClient).mockResolvedValue({
        stop: vi.fn(),
        gotoDefinition: vi.fn().mockResolvedValue([
          {
            uri: defsPath,
            range: {
              start: { line: 1, character: 0 },
              end: { line: 1, character: 3 },
            },
            content: 'ORIGINAL_CONTENT',
          },
        ]),
      } as any);

      vi.mocked(fs.readFile).mockImplementation(async p => {
        const filePath = typeof p === 'string' ? p : String(p);
        if (filePath === testPath) {
          return 'function test() { return 1; }';
        }
        if (filePath === defsPath) return 'alpha\nbeta\ngamma';
        throw new Error(`Unexpected path: ${filePath}`);
      });

      const handler = createHandler();
      const result = await handler({
        queries: [
          {
            uri: testPath,
            symbolName: 'test',
            lineHint: 1,
            contextLines: 1,
            researchGoal: 'Find def',
            reasoning: 'Testing LSP enhanced snippet',
          },
        ],
      });

      const text = result.content?.[0]?.text ?? '';
      expect(text).not.toContain('status: "hasResults"');
      expect(text).toContain('>   2| beta');
    });

    it('should fall back to raw location when reading definition file fails', async () => {
      process.env.WORKSPACE_ROOT = process.cwd();
      const testPath = `${process.cwd()}/src/test.ts`;
      const missingPath = `${process.cwd()}/src/missing.ts`;

      vi.mocked(managerModule.isLanguageServerAvailable).mockResolvedValue(
        true
      );
      vi.mocked(managerModule.acquirePooledClient).mockResolvedValue({
        stop: vi.fn(),
        gotoDefinition: vi.fn().mockResolvedValue([
          {
            uri: missingPath,
            range: {
              start: { line: 0, character: 0 },
              end: { line: 0, character: 3 },
            },
            content: 'RAW_LOC_CONTENT',
          },
        ]),
      } as any);

      vi.mocked(fs.readFile).mockImplementation(async p => {
        const filePath = typeof p === 'string' ? p : String(p);
        if (filePath === testPath) {
          return 'function test() { return 1; }';
        }
        throw new Error('ENOENT');
      });

      const handler = createHandler();
      const result = await handler({
        queries: [
          {
            uri: testPath,
            symbolName: 'test',
            lineHint: 1,
            researchGoal: 'Find def',
            reasoning: 'Testing LSP location fallback',
          },
        ],
      });

      expect(result.content?.[0]?.text ?? '').toContain('RAW_LOC_CONTENT');
    });
  });

  describe('Schema Exports', () => {
    it('should export BulkLSPGotoDefinitionSchema', async () => {
      const { BulkLSPGotoDefinitionSchema } =
        await import('@octocodeai/octocode-core');

      expect(BulkLSPGotoDefinitionSchema).toBeDefined();
    });

    it('should export LSP_GOTO_DEFINITION_DESCRIPTION', async () => {
      const { LSP_GOTO_DEFINITION_DESCRIPTION } =
        await import('@octocodeai/octocode-core');

      expect(LSP_GOTO_DEFINITION_DESCRIPTION).toBeDefined();
      expect(typeof LSP_GOTO_DEFINITION_DESCRIPTION).toBe('string');
    });
  });

  describe('Fallback Behavior', () => {
    it('should use symbol resolver as fallback', async () => {
      vi.mocked(managerModule.isLanguageServerAvailable).mockResolvedValue(
        false
      );

      const handler = createHandler();
      const result = await handler({
        queries: [
          {
            uri: '/workspace/test.ts',
            symbolName: 'testFunction',
            lineHint: 4,
            researchGoal: 'Find def',
            reasoning: 'Testing fallback',
          },
        ],
      });

      expect(result).toBeDefined();
      expect(result.content?.length).toBeGreaterThan(0);
    });

    it('returns an LSP-empty result when an available LSP throws', async () => {
      process.env.WORKSPACE_ROOT = process.cwd();
      const testPath = `${process.cwd()}/src/test.ts`;
      vi.mocked(managerModule.isLanguageServerAvailable).mockResolvedValue(
        true
      );
      vi.mocked(managerModule.acquirePooledClient).mockResolvedValue({
        gotoDefinition: vi.fn().mockRejectedValue(new Error('tsserver boom')),
      } as any);

      const handler = createHandler();
      const result = await handler({
        queries: [
          {
            uri: testPath,
            symbolName: 'testFunction',
            lineHint: 4,
            researchGoal: 'Find def',
            reasoning: 'Testing LSP-empty on thrown LSP',
          },
        ],
      });

      const text = result.content?.[0]?.text ?? '';
      expect(text).toContain('status: "empty"');
      expect(text).toContain('errorCode: "LSP_EMPTY"');
      expect(text).not.toContain('lspMode');
    });
  });

  describe('Dynamic Import Detection (isDynamicImport)', () => {
    let isDynamicImport: typeof import('../../src/tools/lsp_goto_definition/execution.js').isDynamicImport;

    beforeEach(async () => {
      ({ isDynamicImport } =
        await import('../../src/tools/lsp_goto_definition/execution.js'));
    });

    it('should detect simple dynamic import', () => {
      expect(isDynamicImport("const mod = await import('./module')")).toBe(
        true
      );
    });

    it('should detect destructured dynamic import', () => {
      expect(
        isDynamicImport(
          "const { registerTools } = await import('./tools/toolsManager.js')"
        )
      ).toBe(true);
    });

    it('should detect dynamic import with double quotes', () => {
      expect(isDynamicImport('const mod = await import("./module")')).toBe(
        true
      );
    });

    it('should detect dynamic import without await', () => {
      expect(isDynamicImport("const mod = import('./module')")).toBe(true);
    });

    it('should detect dynamic import with .then()', () => {
      expect(
        isDynamicImport("import('./module').then(mod => mod.default)")
      ).toBe(true);
    });

    it('should NOT detect static import', () => {
      expect(isDynamicImport("import { foo } from './module'")).toBe(false);
    });

    it('should NOT detect require()', () => {
      expect(isDynamicImport("const x = require('./module')")).toBe(false);
    });

    it('should NOT detect regular function call with import in name', () => {
      expect(isDynamicImport("const x = importModule('./module')")).toBe(false);
    });

    it('should handle empty string', () => {
      expect(isDynamicImport('')).toBe(false);
    });
  });

  describe('Dynamic Import Chaining via LSP', () => {
    it('should chain through dynamic import when LSP resolves to source in a different file', async () => {
      process.env.WORKSPACE_ROOT = process.cwd();
      const testPath = `${process.cwd()}/src/test.ts`;
      const modulePath = `${process.cwd()}/src/toolsManager.ts`;

      vi.mocked(resolverModule.SymbolResolver).mockImplementation(function () {
        return {
          resolvePositionFromContent: vi.fn().mockReturnValue({
            position: { line: 3, character: 10 },
            foundAtLine: 4,
          }),
          extractContext: vi.fn().mockReturnValue({
            content: 'test content',
            startLine: 1,
            endLine: 10,
          }),
        };
      });

      const mockGotoDefinition = vi.fn();
      mockGotoDefinition.mockResolvedValueOnce([
        {
          uri: testPath,
          range: {
            start: { line: 3, character: 0 },
            end: { line: 3, character: 65 },
          },
          content:
            "const { registerTools } = await import('./toolsManager.js')",
        },
      ]);
      mockGotoDefinition.mockResolvedValueOnce([
        {
          uri: modulePath,
          range: {
            start: { line: 1, character: 0 },
            end: { line: 1, character: 20 },
          },
          content: 'export async function registerTools() { return {}; }',
        },
      ]);

      vi.mocked(managerModule.isLanguageServerAvailable).mockResolvedValue(
        true
      );
      vi.mocked(managerModule.acquirePooledClient).mockResolvedValue({
        stop: vi.fn(),
        gotoDefinition: mockGotoDefinition,
      } as any);

      vi.mocked(fs.readFile).mockImplementation(async p => {
        const path = typeof p === 'string' ? p : String(p);
        if (path === testPath) {
          return "import { Server } from 'http';\n\nasync function main() {\n  const { registerTools } = await import('./toolsManager.js');\n  registerTools();\n}";
        }
        if (path === modulePath) {
          return 'const a = 1;\nexport async function registerTools() { return {}; }\n';
        }
        throw new Error(`Unexpected path: ${path}`);
      });

      const handler = createHandler();
      const result = await handler({
        queries: [
          {
            uri: testPath,
            symbolName: 'registerTools',
            lineHint: 4,
            contextLines: 0,
            researchGoal: 'Find registerTools definition',
            reasoning: 'Testing dynamic import chaining',
          },
        ],
      });

      const text = result.content?.[0]?.text ?? '';
      expect(text).toContain(modulePath);
      expect(text).toContain('Followed import chain to source definition');
    });

    it('does NOT manually resolve module paths when LSP chaining returns empty', async () => {
      process.env.WORKSPACE_ROOT = process.cwd();
      const testPath = `${process.cwd()}/src/test.ts`;
      const modulePath = `${process.cwd()}/src/toolsManager.ts`;

      vi.mocked(resolverModule.SymbolResolver).mockImplementation(function () {
        return {
          resolvePositionFromContent: vi.fn().mockReturnValue({
            position: { line: 0, character: 8 },
            foundAtLine: 1,
          }),
          extractContext: vi.fn().mockReturnValue({
            content: 'test content',
            startLine: 1,
            endLine: 2,
          }),
        };
      });

      vi.mocked(managerModule.isLanguageServerAvailable).mockResolvedValue(
        true
      );
      vi.mocked(managerModule.acquirePooledClient).mockResolvedValue({
        stop: vi.fn(),
        gotoDefinition: vi.fn().mockResolvedValue([]),
      } as any);

      vi.mocked(fs.readFile).mockImplementation(async p => {
        const path = typeof p === 'string' ? p : String(p);
        if (path === testPath) {
          return "const { registerTools } = await import('./toolsManager.js');\nregisterTools();";
        }
        if (path === modulePath) {
          return 'export async function registerTools() { return {}; }\n';
        }
        throw new Error(`Unexpected path: ${path}`);
      });

      const handler = createHandler();
      const result = await handler({
        queries: [
          {
            uri: testPath,
            symbolName: 'registerTools',
            lineHint: 1,
            contextLines: 0,
            researchGoal: 'Find registerTools from dynamic import',
            reasoning: 'Manual module-path fallback was removed (LSP-only)',
          },
        ],
      });

      const text = result.content?.[0]?.text ?? '';
      expect(text).toContain('status: "empty"');
      expect(text).not.toContain('Followed import chain');
    });
  });

  describe('Import Chain Detection (isImportOrReExport)', () => {
    it('should detect named import', () => {
      expect(isImportOrReExport("import { Foo } from './module'")).toBe(true);
    });

    it('should detect default import', () => {
      expect(isImportOrReExport("import Foo from './module'")).toBe(true);
    });

    it('should detect namespace import', () => {
      expect(isImportOrReExport("import * as Foo from './module'")).toBe(true);
    });

    it('should detect named re-export', () => {
      expect(isImportOrReExport("export { Foo } from './module'")).toBe(true);
    });

    it('should detect wildcard re-export', () => {
      expect(isImportOrReExport("export * from './module'")).toBe(true);
    });

    it('should detect re-export with rename', () => {
      expect(
        isImportOrReExport("export { default as Foo } from './module'")
      ).toBe(true);
    });

    it('should detect import with double quotes', () => {
      expect(isImportOrReExport('import { Foo } from "./module"')).toBe(true);
    });

    it('should detect import with .js extension', () => {
      expect(
        isImportOrReExport("import { ToolError } from './ToolError.js'")
      ).toBe(true);
    });

    it('should NOT detect regular export', () => {
      expect(isImportOrReExport('export function foo() {}')).toBe(false);
    });

    it('should NOT detect regular variable', () => {
      expect(isImportOrReExport('const foo = 1;')).toBe(false);
    });

    it('should NOT detect class definition', () => {
      expect(isImportOrReExport('export class Foo {}')).toBe(false);
    });

    it('should NOT detect interface', () => {
      expect(isImportOrReExport('export interface Foo {}')).toBe(false);
    });

    it('should handle leading whitespace', () => {
      expect(isImportOrReExport("  import { Foo } from './module'")).toBe(true);
    });

    it('should handle empty string', () => {
      expect(isImportOrReExport('')).toBe(false);
    });
  });

  describe('Import Chaining via LSP', () => {
    it('should chain through import when LSP resolves to same-file import', async () => {
      process.env.WORKSPACE_ROOT = process.cwd();
      const testPath = `${process.cwd()}/src/test.ts`;
      const sourcePath = `${process.cwd()}/src/source.ts`;

      const mockGotoDefinition = vi.fn();
      mockGotoDefinition.mockResolvedValueOnce([
        {
          uri: testPath,
          range: {
            start: { line: 0, character: 0 },
            end: { line: 0, character: 40 },
          },
          content: "import { Foo } from './source'",
        },
      ]);
      mockGotoDefinition.mockResolvedValueOnce([
        {
          uri: sourcePath,
          range: {
            start: { line: 5, character: 0 },
            end: { line: 5, character: 20 },
          },
          content: 'export class Foo {}',
        },
      ]);

      vi.mocked(managerModule.isLanguageServerAvailable).mockResolvedValue(
        true
      );
      vi.mocked(managerModule.acquirePooledClient).mockResolvedValue({
        stop: vi.fn(),
        gotoDefinition: mockGotoDefinition,
      } as any);

      vi.mocked(fs.readFile).mockImplementation(async p => {
        const path = typeof p === 'string' ? p : String(p);
        if (path === testPath) {
          return "import { Foo } from './source'\n\nconst x = new Foo();";
        }
        if (path === sourcePath) {
          return 'const a = 1;\nconst b = 2;\nconst c = 3;\nconst d = 4;\nconst e = 5;\nexport class Foo {}\nconst f = 6;';
        }
        throw new Error(`Unexpected: ${path}`);
      });

      const handler = createHandler();
      const result = await handler({
        queries: [
          {
            uri: testPath,
            symbolName: 'Foo',
            lineHint: 3,
            contextLines: 1,
            researchGoal: 'Find Foo definition',
            reasoning: 'Testing import chaining',
          },
        ],
      });

      const text = result.content?.[0]?.text ?? '';
      expect(text).toContain(sourcePath);
      expect(text).toContain('Followed import chain to source definition');
      expect(mockGotoDefinition).toHaveBeenCalledTimes(2);
      expect(mockGotoDefinition).toHaveBeenNthCalledWith(2, testPath, {
        line: 0,
        character: 9,
      });
    });

    it('should NOT chain when result is in a different file', async () => {
      process.env.WORKSPACE_ROOT = process.cwd();
      const testPath = `${process.cwd()}/src/test.ts`;
      const defsPath = `${process.cwd()}/src/defs.ts`;

      const mockGotoDefinition = vi.fn().mockResolvedValue([
        {
          uri: defsPath,
          range: {
            start: { line: 5, character: 0 },
            end: { line: 5, character: 20 },
          },
          content: 'export function helper() {}',
        },
      ]);

      vi.mocked(managerModule.isLanguageServerAvailable).mockResolvedValue(
        true
      );
      vi.mocked(managerModule.acquirePooledClient).mockResolvedValue({
        stop: vi.fn(),
        gotoDefinition: mockGotoDefinition,
      } as any);

      vi.mocked(fs.readFile).mockImplementation(async p => {
        const path = typeof p === 'string' ? p : String(p);
        if (path === testPath) return 'const x = helper();';
        if (path === defsPath)
          return 'a\nb\nc\nd\ne\nexport function helper() {}\nf';
        throw new Error(`Unexpected: ${path}`);
      });

      const handler = createHandler();
      const result = await handler({
        queries: [
          {
            uri: testPath,
            symbolName: 'helper',
            lineHint: 1,
            contextLines: 1,
            researchGoal: 'Find helper def',
            reasoning: 'Testing no-chain for different file',
          },
        ],
      });

      const text = result.content?.[0]?.text ?? '';
      expect(text).toContain(defsPath);
      expect(text).not.toContain('Followed import chain');
      expect(mockGotoDefinition).toHaveBeenCalledTimes(1);
    });

    it('should NOT chain when result line is not an import', async () => {
      process.env.WORKSPACE_ROOT = process.cwd();
      const testPath = `${process.cwd()}/src/test.ts`;

      const mockGotoDefinition = vi.fn().mockResolvedValue([
        {
          uri: testPath,
          range: {
            start: { line: 2, character: 0 },
            end: { line: 2, character: 30 },
          },
          content: 'const localVar = 42;',
        },
      ]);

      vi.mocked(managerModule.isLanguageServerAvailable).mockResolvedValue(
        true
      );
      vi.mocked(managerModule.acquirePooledClient).mockResolvedValue({
        stop: vi.fn(),
        gotoDefinition: mockGotoDefinition,
      } as any);

      vi.mocked(fs.readFile).mockImplementation(async () => {
        return 'const a = 1;\nconst b = 2;\nconst localVar = 42;\nconst d = 4;';
      });

      const handler = createHandler();
      const result = await handler({
        queries: [
          {
            uri: testPath,
            symbolName: 'localVar',
            lineHint: 3,
            contextLines: 1,
            researchGoal: 'Find localVar',
            reasoning: 'Testing no-chain for non-import',
          },
        ],
      });

      const text = result.content?.[0]?.text ?? '';
      expect(text).not.toContain('Followed import chain');
      expect(mockGotoDefinition).toHaveBeenCalledTimes(1);
    });

    it('should fallback to original when second hop returns empty', async () => {
      process.env.WORKSPACE_ROOT = process.cwd();
      const testPath = `${process.cwd()}/src/test.ts`;

      const mockGotoDefinition = vi.fn();
      mockGotoDefinition.mockResolvedValueOnce([
        {
          uri: testPath,
          range: {
            start: { line: 0, character: 0 },
            end: { line: 0, character: 35 },
          },
          content: "import { Bar } from './bar'",
        },
      ]);
      mockGotoDefinition.mockResolvedValueOnce([]);

      vi.mocked(managerModule.isLanguageServerAvailable).mockResolvedValue(
        true
      );
      vi.mocked(managerModule.acquirePooledClient).mockResolvedValue({
        stop: vi.fn(),
        gotoDefinition: mockGotoDefinition,
      } as any);

      vi.mocked(fs.readFile).mockImplementation(async () => {
        return "import { Bar } from './bar'\nconst x = new Bar();";
      });

      const handler = createHandler();
      const result = await handler({
        queries: [
          {
            uri: testPath,
            symbolName: 'Bar',
            lineHint: 2,
            contextLines: 0,
            researchGoal: 'Find Bar',
            reasoning: 'Testing fallback on empty chain',
          },
        ],
      });

      const text = result.content?.[0]?.text ?? '';
      expect(text).not.toContain('status: "hasResults"');
      expect(text).not.toContain('Followed import chain');
      expect(mockGotoDefinition).toHaveBeenCalledTimes(2);
    });

    it('should use fallback character when symbol is not found in import line', async () => {
      process.env.WORKSPACE_ROOT = process.cwd();
      const testPath = `${process.cwd()}/src/test.ts`;

      const mockGotoDefinition = vi.fn();
      mockGotoDefinition.mockResolvedValueOnce([
        {
          uri: testPath,
          range: {
            start: { line: 0, character: 4 },
            end: { line: 0, character: 38 },
          },
          content: "import { Bar } from './bar'",
        },
      ]);
      mockGotoDefinition.mockResolvedValueOnce([]);

      vi.mocked(managerModule.isLanguageServerAvailable).mockResolvedValue(
        true
      );
      vi.mocked(managerModule.acquirePooledClient).mockResolvedValue({
        stop: vi.fn(),
        gotoDefinition: mockGotoDefinition,
      } as any);

      vi.mocked(fs.readFile).mockImplementation(async () => {
        return "import { Bar } from './bar'\nconst x = new Bar();";
      });

      const handler = createHandler();
      await handler({
        queries: [
          {
            uri: testPath,
            symbolName: 'NotInImport',
            lineHint: 2,
            contextLines: 0,
            researchGoal: 'Find unknown symbol',
            reasoning: 'Testing fallback character behavior',
          },
        ],
      });

      expect(mockGotoDefinition).toHaveBeenCalledTimes(2);
      expect(mockGotoDefinition).toHaveBeenNthCalledWith(2, testPath, {
        line: 0,
        character: 4,
      });
    });

    it('should fallback when second hop resolves to same file', async () => {
      process.env.WORKSPACE_ROOT = process.cwd();
      const testPath = `${process.cwd()}/src/test.ts`;

      const mockGotoDefinition = vi.fn();
      mockGotoDefinition.mockResolvedValueOnce([
        {
          uri: testPath,
          range: {
            start: { line: 0, character: 0 },
            end: { line: 0, character: 35 },
          },
          content: "import { Baz } from './baz'",
        },
      ]);
      mockGotoDefinition.mockResolvedValueOnce([
        {
          uri: testPath,
          range: {
            start: { line: 0, character: 0 },
            end: { line: 0, character: 35 },
          },
          content: "import { Baz } from './baz'",
        },
      ]);

      vi.mocked(managerModule.isLanguageServerAvailable).mockResolvedValue(
        true
      );
      vi.mocked(managerModule.acquirePooledClient).mockResolvedValue({
        stop: vi.fn(),
        gotoDefinition: mockGotoDefinition,
      } as any);

      vi.mocked(fs.readFile).mockImplementation(async () => {
        return "import { Baz } from './baz'\nconst x = Baz;";
      });

      const handler = createHandler();
      const result = await handler({
        queries: [
          {
            uri: testPath,
            symbolName: 'Baz',
            lineHint: 2,
            contextLines: 0,
            researchGoal: 'Find Baz',
            reasoning: 'Testing loop prevention',
          },
        ],
      });

      const text = result.content?.[0]?.text ?? '';
      expect(text).not.toContain('Followed import chain');
      expect(mockGotoDefinition).toHaveBeenCalledTimes(2);
    });
  });
});
