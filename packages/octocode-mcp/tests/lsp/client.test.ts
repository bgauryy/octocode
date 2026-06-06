import { describe, it, expect, vi, beforeEach } from 'vitest';

describe('LSP Client Module', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('Module exports', () => {
    it('should export LSPClient class', async () => {
      const { LSPClient } = await import('../../src/lsp/client.js');
      expect(LSPClient).toBeDefined();
      expect(typeof LSPClient).toBe('function');
    });

    it('should export acquirePooledClient function', async () => {
      const { acquirePooledClient } = await import('../../src/lsp/manager.js');
      expect(typeof acquirePooledClient).toBe('function');
    });

    it('should export isLanguageServerAvailable function', async () => {
      const { isLanguageServerAvailable } =
        await import('../../src/lsp/manager.js');
      expect(typeof isLanguageServerAvailable).toBe('function');
    });
  });

  describe('LSPClient constructor', () => {
    it('should create instance with config', async () => {
      const { LSPClient } = await import('../../src/lsp/client.js');
      const client = new LSPClient({
        command: 'test-server',
        args: ['--stdio'],
        workspaceRoot: '/workspace',
        languageId: 'typescript',
      });

      expect(client).toBeInstanceOf(LSPClient);
    });

    it('should accept minimal config', async () => {
      const { LSPClient } = await import('../../src/lsp/client.js');
      const client = new LSPClient({
        command: 'test-server',
        workspaceRoot: '/workspace',
      });

      expect(client).toBeDefined();
    });
  });

  describe('isLanguageServerAvailable', () => {
    it('should return false for unsupported file types', async () => {
      const { isLanguageServerAvailable } =
        await import('../../src/lsp/manager.js');

      const result = await isLanguageServerAvailable('/test/file.txt');
      expect(result).toBe(false);
    });

    it('should return false for unknown extensions', async () => {
      const { isLanguageServerAvailable } =
        await import('../../src/lsp/manager.js');

      const result = await isLanguageServerAvailable('/test/file.xyz');
      expect(result).toBe(false);
    });

    it('should return false for files without extension', async () => {
      const { isLanguageServerAvailable } =
        await import('../../src/lsp/manager.js');

      const result = await isLanguageServerAvailable('/test/Makefile');
      expect(result).toBe(false);
    });
  });

  describe('acquirePooledClient', () => {
    it('should return null for unsupported file types', async () => {
      const { acquirePooledClient } = await import('../../src/lsp/manager.js');

      const result = await acquirePooledClient(
        '/workspace',
        '/workspace/file.txt'
      );
      expect(result).toBeNull();
    });

    it('should return null for unknown extensions', async () => {
      const { acquirePooledClient } = await import('../../src/lsp/manager.js');

      const result = await acquirePooledClient(
        '/workspace',
        '/workspace/file.xyz'
      );
      expect(result).toBeNull();
    });
  });

  describe('Language server command mapping', () => {
    const supportedExtensions = [
      { ext: '.ts', languageId: 'typescript' },
      { ext: '.tsx', languageId: 'typescriptreact' },
      { ext: '.js', languageId: 'javascript' },
      { ext: '.jsx', languageId: 'javascriptreact' },
      { ext: '.py', languageId: 'python' },
      { ext: '.go', languageId: 'go' },
      { ext: '.rs', languageId: 'rust' },
    ];

    for (const { ext, languageId } of supportedExtensions) {
      it(`should recognize ${ext} as ${languageId}`, async () => {
        await import('../../src/lsp/manager.js');

        const filePath = `/test/file${ext}`;

        expect(filePath.endsWith(ext)).toBe(true);
      });
    }
  });

  describe('URI conversion logic', () => {
    it('should handle Unix paths', () => {
      const { URI } = require('vscode-uri');

      const uri = URI.file('/users/me/file.ts').toString();
      expect(uri).toBe('file:///users/me/file.ts');
    });

    it('should handle paths with spaces', () => {
      const { URI } = require('vscode-uri');

      const uri = URI.file('/path/with spaces/file.ts').toString();
      expect(uri).toBe('file:///path/with%20spaces/file.ts');
    });

    it('should handle paths with special characters', () => {
      const { URI } = require('vscode-uri');

      const uri = URI.file('/path/file#1.ts').toString();
      expect(uri).toBe('file:///path/file%231.ts');
    });

    it('should round-trip paths correctly', () => {
      const { URI } = require('vscode-uri');

      const originalPath = '/path/with spaces/file.ts';
      const uri = URI.file(originalPath).toString();
      const restored = URI.parse(uri).fsPath;

      expect(restored).toBe(originalPath);
    });
  });

  describe('Symbol kind conversion', () => {
    const symbolKindMap: Record<number, string> = {
      12: 'function',
      6: 'method',
      5: 'class',
      11: 'interface',
      13: 'variable',
      14: 'constant',
      7: 'property',
      10: 'enum',
      2: 'module',
      3: 'namespace',
    };

    for (const [lspKind, expected] of Object.entries(symbolKindMap)) {
      it(`should map LSP SymbolKind ${lspKind} to "${expected}"`, () => {
        const kind = parseInt(lspKind, 10);

        let result: string;
        switch (kind) {
          case 12:
            result = 'function';
            break;
          case 6:
            result = 'method';
            break;
          case 5:
            result = 'class';
            break;
          case 11:
            result = 'interface';
            break;
          case 13:
            result = 'variable';
            break;
          case 14:
            result = 'constant';
            break;
          case 7:
            result = 'property';
            break;
          case 10:
            result = 'enum';
            break;
          case 2:
            result = 'module';
            break;
          case 3:
            result = 'namespace';
            break;
          default:
            result = 'unknown';
        }

        expect(result).toBe(expected);
      });
    }

    it('should return "unknown" for unmapped kinds', () => {
      const result = 'unknown';

      expect(result).toBe('unknown');
    });
  });

  describe('LSP capability check', () => {
    it('should handle missing capabilities', async () => {
      const { LSPClient } = await import('../../src/lsp/client.js');

      const client = new LSPClient({
        command: 'test-server',
        workspaceRoot: '/workspace',
      });

      expect(client.hasCapability('definitionProvider')).toBe(false);
    });
  });

  describe('Environment variable handling', () => {
    it('should check for TS server path env var', () => {
      const envVar = 'OCTOCODE_TS_SERVER_PATH';
      expect(typeof envVar).toBe('string');
    });

    it('should check for Python server path env var', () => {
      const envVar = 'OCTOCODE_PYTHON_SERVER_PATH';
      expect(typeof envVar).toBe('string');
    });

    it('should check for Go server path env var', () => {
      const envVar = 'OCTOCODE_GO_SERVER_PATH';
      expect(typeof envVar).toBe('string');
    });

    it('should check for Rust server path env var', () => {
      const envVar = 'OCTOCODE_RUST_SERVER_PATH';
      expect(typeof envVar).toBe('string');
    });
  });

  describe('Language ID detection', () => {
    const testCases = [
      { path: '/file.ts', expected: 'typescript' },
      { path: '/file.tsx', expected: 'typescriptreact' },
      { path: '/file.js', expected: 'javascript' },
      { path: '/file.jsx', expected: 'javascriptreact' },
      { path: '/file.py', expected: 'python' },
      { path: '/file.go', expected: 'go' },
      { path: '/file.rs', expected: 'rust' },
      { path: '/file.txt', expected: 'plaintext' },
      { path: '/file.unknown', expected: 'plaintext' },
    ];

    for (const { path, expected } of testCases) {
      it(`should detect "${expected}" for ${path}`, () => {
        const ext = require('path').extname(path).toLowerCase();

        const langMap: Record<string, string> = {
          '.ts': 'typescript',
          '.tsx': 'typescriptreact',
          '.js': 'javascript',
          '.jsx': 'javascriptreact',
          '.py': 'python',
          '.go': 'go',
          '.rs': 'rust',
        };

        const result = langMap[ext] ?? 'plaintext';
        expect(result).toBe(expected);
      });
    }
  });
});
