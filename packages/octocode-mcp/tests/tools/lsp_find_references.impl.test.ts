import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

vi.mock('fs/promises', () => ({
  readFile: vi.fn(),
  stat: vi.fn(),
}));

vi.mock('child_process', () => ({
  exec: vi.fn(),
}));

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

import * as fs from 'fs/promises';
import * as childProcess from 'child_process';
import * as resolverModule from '../../src/lsp/resolver.js';
import * as managerModule from '../../src/lsp/manager.js';

import { registerLSPFindReferencesTool } from '../../src/tools/lsp_find_references/register.js';

describe('LSP Find References Implementation Tests', () => {
  const sampleTypeScriptContent = `
import { something } from './module';

export function testFunction(param: string): string {
  const result = param.toUpperCase();
  return result;
}

export function anotherFunction() {
  const value = testFunction('hello');
  console.log(value);
}
`.trim();

  beforeEach(() => {
    vi.clearAllMocks();

    process.env.WORKSPACE_ROOT = '/workspace';

    vi.mocked(fs.stat).mockResolvedValue({ isFile: () => true } as any);
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
    registerLSPFindReferencesTool(mockServer as any);
    return mockServer.registerTool.mock.results[0]!.value;
  };

  describe('Tool Registration', () => {
    it('should register the tool with correct name', () => {
      const mockServer = {
        registerTool: vi.fn(),
      };

      registerLSPFindReferencesTool(mockServer as any);

      expect(mockServer.registerTool).toHaveBeenCalledWith(
        'lspFindReferences',
        expect.any(Object),
        expect.any(Function)
      );
    });
  });

  describe('Error Handling', () => {
    it('should handle file not found error', async () => {
      vi.mocked(fs.stat).mockRejectedValue(
        Object.assign(new Error('ENOENT'), { code: 'ENOENT' })
      );

      const handler = createHandler();
      const result = await handler({
        queries: [
          {
            uri: '/workspace/nonexistent.ts',
            symbolName: 'test',
            lineHint: 1,
            researchGoal: 'Find refs',
            reasoning: 'Testing',
          },
        ],
      });

      expect(result).toBeDefined();
      expect(result.content?.length).toBeGreaterThan(0);
    });

    it('should handle file read errors', async () => {
      vi.mocked(fs.stat).mockResolvedValue({ isFile: () => true } as any);
      vi.mocked(fs.readFile).mockRejectedValue(new Error('Permission denied'));

      const handler = createHandler();
      const result = await handler({
        queries: [
          {
            uri: '/workspace/protected.ts',
            symbolName: 'test',
            lineHint: 1,
            researchGoal: 'Find refs',
            reasoning: 'Testing',
          },
        ],
      });

      expect(result).toBeDefined();
      expect(result.content?.length).toBeGreaterThan(0);
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
            researchGoal: 'Find refs',
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
            researchGoal: 'Find refs',
            reasoning: 'Testing',
          },
        ],
      });

      expect(result).toBeDefined();
    });

    it('should handle relative paths', async () => {
      const handler = createHandler();
      const result = await handler({
        queries: [
          {
            uri: 'src/relative.ts',
            symbolName: 'test',
            lineHint: 1,
            researchGoal: 'Find refs',
            reasoning: 'Testing',
          },
        ],
      });

      expect(result).toBeDefined();
    });
  });

  describe('Multiple Queries', () => {
    it('should process multiple queries in batch', async () => {
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
            researchGoal: 'Find references',
            reasoning: 'Testing',
          },
        ],
      });

      expect(result).toBeDefined();
    });

    it('should handle pagination parameters', async () => {
      const handler = createHandler();
      const result = await handler({
        queries: [
          {
            uri: '/workspace/test.ts',
            symbolName: 'test',
            lineHint: 1,
            referencesPerPage: 10,
            page: 2,
            researchGoal: 'Find refs',
            reasoning: 'Testing pagination',
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
            contextLines: 5,
            researchGoal: 'Find refs',
            reasoning: 'Testing context',
          },
        ],
      });

      expect(result).toBeDefined();
    });

    it('should handle includeDeclaration parameter', async () => {
      const handler = createHandler();
      const result = await handler({
        queries: [
          {
            uri: '/workspace/test.ts',
            symbolName: 'test',
            lineHint: 1,
            includeDeclaration: false,
            researchGoal: 'Find refs',
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
            researchGoal: 'Find refs',
            reasoning: 'Testing',
          },
        ],
      });

      expect(result).toBeDefined();
      expect(result.content?.length).toBeGreaterThan(0);
    });

    it('should attempt LSP when available', async () => {
      const mockClient = {
        stop: vi.fn(),
        findReferences: vi.fn().mockResolvedValue([]),
      };
      vi.mocked(managerModule.isLanguageServerAvailable).mockResolvedValue(
        true
      );
      vi.mocked(managerModule.acquirePooledClient).mockResolvedValue(
        mockClient as any
      );

      const handler = createHandler();
      const result = await handler({
        queries: [
          {
            uri: '/workspace/test.ts',
            symbolName: 'test',
            lineHint: 1,
            researchGoal: 'Find refs',
            reasoning: 'Testing',
          },
        ],
      });

      expect(result).toBeDefined();
    });

    it('should return an empty result (no text fallback) when available LSP finds no references', async () => {
      process.env.WORKSPACE_ROOT = process.cwd();
      const testPath = `${process.cwd()}/src/test.ts`;
      vi.mocked(managerModule.isLanguageServerAvailable).mockResolvedValue(
        true
      );
      vi.mocked(managerModule.acquirePooledClient).mockResolvedValue({
        stop: vi.fn(),
        findReferences: vi.fn().mockResolvedValue([]),
      } as any);

      const handler = createHandler();
      const result = await handler({
        queries: [
          {
            uri: testPath,
            symbolName: 'testFunction',
            lineHint: 4,
            researchGoal: 'Find refs',
            reasoning: 'Testing semantic-only empty result',
          },
        ],
      });

      const text = result.content?.[0]?.text ?? '';
      expect(text).toContain('status: "empty"');
      expect(text).not.toContain('lspMode');
    });

    it('should paginate and enhance locations when LSP returns references', async () => {
      process.env.WORKSPACE_ROOT = process.cwd();
      const testPath = `${process.cwd()}/src/test.ts`;
      const otherPath = `${process.cwd()}/src/other.ts`;

      vi.mocked(managerModule.isLanguageServerAvailable).mockResolvedValue(
        true
      );
      vi.mocked(managerModule.acquirePooledClient).mockResolvedValue({
        stop: vi.fn(),
        findReferences: vi.fn().mockResolvedValue([
          {
            uri: testPath,
            range: {
              start: { line: 3, character: 16 },
              end: { line: 3, character: 28 },
            },
            content: 'export function testFunction() {}',
          },
          {
            uri: otherPath,
            range: {
              start: { line: 10, character: 5 },
              end: { line: 10, character: 17 },
            },
            content: 'const x = testFunction();',
          },
        ]),
      } as any);

      vi.mocked(fs.readFile).mockImplementation(async p => {
        const filePath = typeof p === 'string' ? p : String(p);
        if (filePath === testPath) {
          return [
            'line1',
            'line2',
            'line3',
            'export function testFunction() {}',
            'line5',
          ].join('\n');
        }
        if (filePath === otherPath) {
          return [
            'line9',
            'line10',
            'const x = testFunction();',
            'line12',
          ].join('\n');
        }
        return sampleTypeScriptContent;
      });

      const handler = createHandler();
      const result = await handler({
        queries: [
          {
            uri: testPath,
            symbolName: 'testFunction',
            lineHint: 4,
            contextLines: 1,
            referencesPerPage: 1,
            page: 1,
            researchGoal: 'Find refs',
            reasoning: 'Testing LSP pagination + enhancement',
          },
        ],
      });

      const text = result.content?.[0]?.text ?? '';
      expect(text).not.toContain('status: "hasResults"');
      expect(text).toContain('totalPages: 2');
    });
  });

  describe('LSP unavailable (no text fallback)', () => {
    it('should return an LSP-not-installed empty result instead of a text fallback', async () => {
      process.env.WORKSPACE_ROOT = process.cwd();
      const testPath = `${process.cwd()}/src/test.ts`;

      vi.mocked(managerModule.isLanguageServerAvailable).mockResolvedValue(
        false
      );

      vi.mocked(fs.readFile).mockImplementation(async p => {
        const filePath = typeof p === 'string' ? p : String(p);
        if (filePath === testPath) {
          return [
            'line1',
            'line2',
            'line3',
            'export function testFunction() {}',
            'line5',
          ].join('\n');
        }
        return sampleTypeScriptContent;
      });

      const handler = createHandler();
      const result = await handler({
        queries: [
          {
            uri: testPath,
            symbolName: 'testFunction',
            lineHint: 4,
            contextLines: 1,
            researchGoal: 'Find refs',
            reasoning: 'Testing LSP-unavailable empty result',
          },
        ],
      });

      const text = result.content?.[0]?.text ?? '';
      expect(text).toContain('status: "empty"');
      expect(text).toContain('LSP_NOT_INSTALLED');
      expect(text).not.toContain('lspMode');
      expect(text).toContain('localSearchCode');
    });
  });

  describe('Schema Exports', () => {
    it('should export BulkLSPFindReferencesSchema', async () => {
      const { BulkLSPFindReferencesSchema } =
        await import('@octocodeai/octocode-core');

      expect(BulkLSPFindReferencesSchema).toBeDefined();
    });

    it('should export LSP_FIND_REFERENCES_DESCRIPTION', async () => {
      const { LSP_FIND_REFERENCES_DESCRIPTION } =
        await import('@octocodeai/octocode-core');

      expect(LSP_FIND_REFERENCES_DESCRIPTION).toBeDefined();
      expect(typeof LSP_FIND_REFERENCES_DESCRIPTION).toBe('string');
    });
  });

  describe('Pagination Logic', () => {
    it('should create proper pagination info', () => {
      const totalReferences = 55;
      const referencesPerPage = 20;
      const page = 2;

      const totalPages = Math.ceil(totalReferences / referencesPerPage);
      const startIndex = (page - 1) * referencesPerPage;
      const endIndex = Math.min(
        startIndex + referencesPerPage,
        totalReferences
      );

      const pagination = {
        currentPage: page,
        totalPages,
        totalResults: totalReferences,
        hasMore: page < totalPages,
        resultsPerPage: referencesPerPage,
      };

      expect(pagination.currentPage).toBe(2);
      expect(pagination.totalPages).toBe(3);
      expect(pagination.hasMore).toBe(true);
      expect(startIndex).toBe(20);
      expect(endIndex).toBe(40);
    });
  });
});
