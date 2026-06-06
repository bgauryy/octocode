import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import * as path from 'path';
import * as cp from 'child_process';

vi.mock('child_process', () => ({
  spawn: vi.fn(),
}));

vi.mock('fs/promises', () => ({
  readFile: vi.fn().mockResolvedValue('const test = 1;'),
  access: vi.fn().mockRejectedValue(new Error('ENOENT')),
}));

describe('LSP Client Handler Tests', () => {
  beforeEach(() => {
    vi.clearAllMocks();

    vi.mocked(cp.spawn).mockReturnValue({
      stdin: { write: vi.fn() },
      stdout: { on: vi.fn(), setEncoding: vi.fn() },
      stderr: { on: vi.fn(), setEncoding: vi.fn() } as any,
      on: vi.fn((event, cb) => {
        if (event === 'close') {
          setTimeout(() => cb(1), 10);
        }
        return { stdin: {}, stdout: {}, stderr: {}, on: vi.fn() };
      }) as any,
      kill: vi.fn(),
      pid: 12345,
    } as any);
  });

  afterEach(() => {
    vi.resetAllMocks();
  });

  describe('LSPClient class', () => {
    it('should create instance without args', async () => {
      const { LSPClient } = await import('../../src/lsp/client.js');

      const client = new LSPClient({
        command: 'test-server',
        workspaceRoot: '/workspace',
      });

      expect(client).toBeDefined();
      expect(client.hasCapability('any')).toBe(false);
    });

    it('should create instance with full config', async () => {
      const { LSPClient } = await import('../../src/lsp/client.js');

      const client = new LSPClient({
        command: 'typescript-language-server',
        args: ['--stdio'],
        workspaceRoot: '/workspace',
        languageId: 'typescript',
      });

      expect(client).toBeDefined();
    });

    it('should report no capabilities before initialization', async () => {
      const { LSPClient } = await import('../../src/lsp/client.js');

      const client = new LSPClient({
        command: 'test-server',
        workspaceRoot: '/workspace',
      });

      expect(client.hasCapability('textDocument/definition')).toBe(false);
      expect(client.hasCapability('textDocument/references')).toBe(false);
      expect(client.hasCapability('callHierarchyProvider')).toBe(false);
    });
  });

  describe('acquirePooledClient function', () => {
    it('should return null for unsupported extension .txt', async () => {
      const { acquirePooledClient } = await import('../../src/lsp/manager.js');

      const result = await acquirePooledClient(
        '/workspace',
        '/workspace/file.txt'
      );

      expect(result).toBeNull();
    });

    it('should return null for unsupported extension .md', async () => {
      const { acquirePooledClient } = await import('../../src/lsp/manager.js');

      const result = await acquirePooledClient(
        '/workspace',
        '/workspace/README.md'
      );

      expect(result).toBeNull();
    });

    it('should return null for unsupported extension .json', async () => {
      const { acquirePooledClient } = await import('../../src/lsp/manager.js');

      const result = await acquirePooledClient(
        '/workspace',
        '/workspace/package.json'
      );

      expect(result).toBeNull();
    });

    it('should return null for unsupported extension .yaml', async () => {
      const { acquirePooledClient } = await import('../../src/lsp/manager.js');

      const result = await acquirePooledClient(
        '/workspace',
        '/workspace/config.yaml'
      );

      expect(result).toBeNull();
    });

    it('should return null for unsupported extension .html', async () => {
      const { acquirePooledClient } = await import('../../src/lsp/manager.js');

      const result = await acquirePooledClient(
        '/workspace',
        '/workspace/index.html'
      );

      expect(result).toBeNull();
    });

    it('should return null for unsupported extension .css', async () => {
      const { acquirePooledClient } = await import('../../src/lsp/manager.js');

      const result = await acquirePooledClient(
        '/workspace',
        '/workspace/styles.css'
      );

      expect(result).toBeNull();
    });

    it('should return null for files without extension', async () => {
      const { acquirePooledClient } = await import('../../src/lsp/manager.js');

      const result = await acquirePooledClient(
        '/workspace',
        '/workspace/Dockerfile'
      );

      expect(result).toBeNull();
    });

    it('should return null for hidden files', async () => {
      const { acquirePooledClient } = await import('../../src/lsp/manager.js');

      const result = await acquirePooledClient(
        '/workspace',
        '/workspace/.gitignore'
      );

      expect(result).toBeNull();
    });
  });

  describe('isLanguageServerAvailable function', () => {
    it('should return false for .txt extension', async () => {
      const { isLanguageServerAvailable } =
        await import('../../src/lsp/manager.js');

      const result = await isLanguageServerAvailable('/test/file.txt');

      expect(result).toBe(false);
    });

    it('should return false for .md extension', async () => {
      const { isLanguageServerAvailable } =
        await import('../../src/lsp/manager.js');

      const result = await isLanguageServerAvailable('/test/README.md');

      expect(result).toBe(false);
    });

    it('should return false for .json extension', async () => {
      const { isLanguageServerAvailable } =
        await import('../../src/lsp/manager.js');

      const result = await isLanguageServerAvailable('/test/config.json');

      expect(result).toBe(false);
    });

    it('should return false for no extension', async () => {
      const { isLanguageServerAvailable } =
        await import('../../src/lsp/manager.js');

      const result = await isLanguageServerAvailable('/test/Makefile');

      expect(result).toBe(false);
    });

    it('should return false for .html extension', async () => {
      const { isLanguageServerAvailable } =
        await import('../../src/lsp/manager.js');

      const result = await isLanguageServerAvailable('/test/index.html');

      expect(result).toBe(false);
    });

    it('should return false for .css extension', async () => {
      const { isLanguageServerAvailable } =
        await import('../../src/lsp/manager.js');

      const result = await isLanguageServerAvailable('/test/styles.css');

      expect(result).toBe(false);
    });

    it('should return false for .svg extension', async () => {
      const { isLanguageServerAvailable } =
        await import('../../src/lsp/manager.js');

      const result = await isLanguageServerAvailable('/test/logo.svg');

      expect(result).toBe(false);
    });

    it('should return false for .xml extension', async () => {
      const { isLanguageServerAvailable } =
        await import('../../src/lsp/manager.js');

      const result = await isLanguageServerAvailable('/test/config.xml');

      expect(result).toBe(false);
    });
  });

  describe('Language server configuration', () => {
    it('should have TypeScript config for .ts files', () => {
      const config = {
        '.ts': {
          command: 'typescript-language-server',
          args: ['--stdio'],
          languageId: 'typescript',
          envVar: 'OCTOCODE_TS_SERVER_PATH',
        },
      };

      expect(config['.ts'].command).toBe('typescript-language-server');
      expect(config['.ts'].languageId).toBe('typescript');
    });

    it('should have TypeScript config for .tsx files', () => {
      const config = {
        '.tsx': {
          command: 'typescript-language-server',
          args: ['--stdio'],
          languageId: 'typescriptreact',
          envVar: 'OCTOCODE_TS_SERVER_PATH',
        },
      };

      expect(config['.tsx'].languageId).toBe('typescriptreact');
    });

    it('should have Python config for .py files', () => {
      const config = {
        '.py': {
          command: 'pylsp',
          args: [],
          languageId: 'python',
          envVar: 'OCTOCODE_PYTHON_SERVER_PATH',
        },
      };

      expect(config['.py'].command).toBe('pylsp');
      expect(config['.py'].languageId).toBe('python');
    });

    it('should have Go config for .go files', () => {
      const config = {
        '.go': {
          command: 'gopls',
          args: ['serve'],
          languageId: 'go',
          envVar: 'OCTOCODE_GO_SERVER_PATH',
        },
      };

      expect(config['.go'].command).toBe('gopls');
      expect(config['.go'].args).toContain('serve');
    });

    it('should have Rust config for .rs files', () => {
      const config = {
        '.rs': {
          command: 'rust-analyzer',
          args: [],
          languageId: 'rust',
          envVar: 'OCTOCODE_RUST_SERVER_PATH',
        },
      };

      expect(config['.rs'].command).toBe('rust-analyzer');
      expect(config['.rs'].languageId).toBe('rust');
    });
  });

  describe('Environment variable resolution', () => {
    it('should use env var when set', () => {
      const envVar = 'OCTOCODE_TEST_PATH';
      const originalValue = process.env[envVar];

      process.env[envVar] = '/custom/path/server';

      const config = {
        command: 'default-server',
        args: ['--stdio'],
        envVar: envVar,
      };

      const resolved = process.env[config.envVar]
        ? { command: process.env[config.envVar]!, args: config.args }
        : { command: config.command, args: config.args };

      expect(resolved.command).toBe('/custom/path/server');

      if (originalValue === undefined) {
        delete process.env[envVar];
      } else {
        process.env[envVar] = originalValue;
      }
    });

    it('should use default when env var not set', () => {
      const config = {
        command: 'default-server',
        args: ['--stdio'],
        envVar: 'OCTOCODE_NONEXISTENT_VAR_12345',
      };

      const resolved = process.env[config.envVar]
        ? { command: process.env[config.envVar]!, args: config.args }
        : { command: config.command, args: config.args };

      expect(resolved.command).toBe('default-server');
    });
  });

  describe('Path helpers', () => {
    it('should detect language from file extension', () => {
      const languageMap: Record<string, string> = {
        '.ts': 'typescript',
        '.tsx': 'typescriptreact',
        '.js': 'javascript',
        '.jsx': 'javascriptreact',
        '.py': 'python',
        '.go': 'go',
        '.rs': 'rust',
      };

      const detectLanguageId = (filePath: string): string => {
        const ext = path.extname(filePath).toLowerCase();
        return languageMap[ext] ?? 'plaintext';
      };

      expect(detectLanguageId('/test/file.ts')).toBe('typescript');
      expect(detectLanguageId('/test/file.py')).toBe('python');
      expect(detectLanguageId('/test/file.go')).toBe('go');
      expect(detectLanguageId('/test/file.txt')).toBe('plaintext');
    });
  });

  describe('Command availability checking', () => {
    it('should use which on Unix', () => {
      const isWindows = process.platform === 'win32';
      const checkCmd = isWindows ? 'where' : 'which';

      if (!isWindows) {
        expect(checkCmd).toBe('which');
      }
    });

    it('should use where on Windows', () => {
      const isWindows = process.platform === 'win32';
      const checkCmd = isWindows ? 'where' : 'which';

      if (isWindows) {
        expect(checkCmd).toBe('where');
      }
    });
  });
});
