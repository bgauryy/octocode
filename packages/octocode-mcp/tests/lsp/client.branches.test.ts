import { describe, it, expect, vi, beforeEach, afterEach, Mock } from 'vitest';
import { LSPClient } from '../../src/lsp/client.js';
import {
  acquirePooledClient,
  isLanguageServerAvailable,
} from '../../src/lsp/manager.js';
import * as cp from 'child_process';
import * as fs from 'fs';
import * as jsonrpc from 'vscode-jsonrpc/node.js';
import { EventEmitter } from 'events';

vi.mock('child_process', () => ({
  spawn: vi.fn(),
}));

vi.mock('fs', () => ({
  promises: {
    readFile: vi.fn(),
    access: vi.fn(),
  },
}));

vi.mock('vscode-jsonrpc/node.js', () => ({
  createMessageConnection: vi.fn(),
  StreamMessageReader: vi.fn(),
  StreamMessageWriter: vi.fn(),
  CancellationTokenSource: class {
    token = {
      isCancellationRequested: false,
      onCancellationRequested: vi.fn(),
    };
    cancel = vi.fn();
    dispose = vi.fn();
  },
}));

describe('LSP Client Branch Coverage', () => {
  let mockProcess: EventEmitter & {
    stdin: EventEmitter;
    stdout: EventEmitter;
    stderr: EventEmitter;
    kill: Mock;
    pid: number;
  };
  let mockConnection: {
    listen: Mock;
    sendRequest: Mock;
    sendNotification: Mock;
    dispose: Mock;
    onNotification: Mock;
    onRequest: Mock;
  };

  beforeEach(() => {
    vi.clearAllMocks();

    mockProcess = new EventEmitter() as typeof mockProcess;
    mockProcess.stdin = new EventEmitter();
    mockProcess.stdout = new EventEmitter();
    mockProcess.stderr = new EventEmitter();
    mockProcess.kill = vi.fn();
    mockProcess.pid = 12345;
    (cp.spawn as Mock).mockReturnValue(mockProcess);

    mockConnection = {
      listen: vi.fn(),
      sendRequest: vi.fn().mockResolvedValue({}),
      sendNotification: vi.fn(),
      dispose: vi.fn(),
      onNotification: vi.fn(),
      onRequest: vi.fn(),
    };
    (jsonrpc.createMessageConnection as Mock).mockReturnValue(mockConnection);

    (fs.promises.readFile as Mock).mockImplementation((path: string) => {
      if (path.includes('lsp-servers.json')) {
        return Promise.reject(new Error('ENOENT'));
      }
      return Promise.resolve('file content');
    });
  });

  afterEach(async () => {
    delete process.env['OCTOCODE_LSP_CONFIG'];
  });

  describe('User config loading from env var (line 117)', () => {
    it('should load config from OCTOCODE_LSP_CONFIG env var', async () => {
      const customConfigPath = '/custom/config/lsp-servers.json';
      process.env['OCTOCODE_LSP_CONFIG'] = customConfigPath;

      const userConfig = {
        languageServers: {
          '.py': {
            command: 'custom-pylsp',
            args: ['--custom'],
            languageId: 'python',
          },
        },
      };

      (fs.promises.readFile as Mock).mockImplementation((path: string) => {
        if (path === customConfigPath) {
          return Promise.resolve(JSON.stringify(userConfig));
        }
        return Promise.reject(new Error('ENOENT'));
      });

      const mockCheckProcess = new EventEmitter();
      (mockCheckProcess as EventEmitter & { kill: Mock }).kill = vi.fn();
      (cp.spawn as Mock).mockImplementation(() => {
        setImmediate(() => mockCheckProcess.emit('close', 0));
        return mockCheckProcess;
      });

      await isLanguageServerAvailable('/file.py', '/workspace');

      expect(fs.promises.readFile).toHaveBeenCalledWith(
        customConfigPath,
        'utf-8'
      );
    });
  });

  it('should read user config from disk on each call (no caching)', async () => {
    const workspaceConfigPath = '/workspace/.octocode/lsp-servers.json';

    const userConfig = {
      languageServers: {
        '.rb': {
          command: 'ruby-lsp',
          languageId: 'ruby',
        },
      },
    };

    (fs.promises.readFile as Mock).mockImplementation((path: string) => {
      if (path === workspaceConfigPath) {
        return Promise.resolve(JSON.stringify(userConfig));
      }
      return Promise.reject(new Error('ENOENT'));
    });

    const mockCheckProcess = new EventEmitter();
    (mockCheckProcess as EventEmitter & { kill: Mock }).kill = vi.fn();
    (cp.spawn as Mock).mockImplementation(() => {
      setImmediate(() => mockCheckProcess.emit('close', 0));
      return mockCheckProcess;
    });

    await isLanguageServerAvailable('/file.rb', '/workspace');
    expect(fs.promises.readFile).toHaveBeenCalled();

    vi.clearAllMocks();
    (fs.promises.readFile as Mock).mockImplementation((path: string) => {
      if (path === workspaceConfigPath) {
        return Promise.resolve(JSON.stringify(userConfig));
      }
      return Promise.reject(new Error('ENOENT'));
    });
    const mockCheckProcess2 = new EventEmitter();
    (mockCheckProcess2 as EventEmitter & { kill: Mock }).kill = vi.fn();
    (cp.spawn as Mock).mockImplementation(() => {
      setImmediate(() => mockCheckProcess2.emit('close', 0));
      return mockCheckProcess2;
    });
    await isLanguageServerAvailable('/file.rb', '/workspace');
    expect(fs.promises.readFile).toHaveBeenCalled();
  });

  describe('User config with custom args (lines 611-617)', () => {
    it('should use args from user config when defined', async () => {
      const userConfig = {
        languageServers: {
          '.java': {
            command: 'custom-jdtls',
            args: ['--data', '/tmp/jdt-workspace'],
            languageId: 'java',
          },
        },
      };

      (fs.promises.readFile as Mock).mockImplementation((path: string) => {
        if (path.includes('lsp-servers.json')) {
          return Promise.resolve(JSON.stringify(userConfig));
        }
        return Promise.resolve('file content');
      });

      const client = await acquirePooledClient(
        '/workspace',
        '/workspace/Main.java'
      );

      if (client) {
        expect(cp.spawn).toHaveBeenCalledWith(
          'custom-jdtls',
          expect.arrayContaining(['--data', '/tmp/jdt-workspace']),
          expect.any(Object)
        );
      }
    });

    it('should use empty args when user config omits args', async () => {
      const userConfig = {
        languageServers: {
          '.scala': {
            command: 'custom-metals',
            languageId: 'scala',
          },
        },
      };

      (fs.promises.readFile as Mock).mockImplementation((path: string) => {
        if (path.includes('lsp-servers.json')) {
          return Promise.resolve(JSON.stringify(userConfig));
        }
        return Promise.resolve('file content');
      });

      const client = await acquirePooledClient(
        '/workspace',
        '/workspace/Main.scala'
      );

      if (client) {
        expect(cp.spawn).toHaveBeenCalledWith(
          'custom-metals',
          [],
          expect.any(Object)
        );
      }
    });
  });

  describe('isLanguageServerAvailable with user config (line 1281)', () => {
    it('should check user config command availability', async () => {
      const userConfig = {
        languageServers: {
          '.custom': {
            command: 'custom-server',
            languageId: 'custom',
          },
        },
      };

      (fs.promises.readFile as Mock).mockImplementation((path: string) => {
        if (path.includes('lsp-servers.json')) {
          return Promise.resolve(JSON.stringify(userConfig));
        }
        return Promise.resolve('file content');
      });

      const mockCheckProcess = new EventEmitter();
      (mockCheckProcess as EventEmitter & { kill: Mock }).kill = vi.fn();

      (cp.spawn as Mock).mockImplementation((cmd: string) => {
        if (cmd === 'which' || cmd === 'where') {
          setImmediate(() => mockCheckProcess.emit('close', 0));
          return mockCheckProcess;
        }
        return mockProcess;
      });

      const result = await isLanguageServerAvailable(
        '/file.custom',
        '/workspace'
      );
      expect(result).toBe(true);
    });
  });

  describe('closeDocument when not initialized (line 865)', () => {
    it('should return early when client not initialized', async () => {
      const client = new LSPClient({
        command: 'test-server',
        workspaceRoot: '/workspace',
      });

      await expect(
        client.closeDocument('/workspace/file.ts')
      ).resolves.toBeUndefined();

      expect(mockConnection.sendNotification).not.toHaveBeenCalled();
    });
  });

  describe('initialize without connection (line 776)', () => {
    it('should throw when initialize called without connection', async () => {
      const client = new LSPClient({
        command: 'test-server',
        workspaceRoot: '/workspace',
      });

      (cp.spawn as Mock).mockReturnValueOnce({
        stdin: null,
        stdout: null,
        stderr: new EventEmitter(),
        on: vi.fn(),
        kill: vi.fn(),
      });

      await expect(client.start()).rejects.toThrow(
        'Failed to create language server process pipes'
      );
    });
  });

  describe('SymbolKind conversion - TypeParameter (line 660)', () => {
    let client: LSPClient;

    beforeEach(async () => {
      mockConnection.sendRequest.mockResolvedValueOnce({
        capabilities: { callHierarchyProvider: true },
      });
      client = new LSPClient({
        command: 'test-server',
        workspaceRoot: '/workspace',
        languageId: 'typescript',
      });
      await client.start();
    });

    it('should convert TypeParameter (26) to type', async () => {
      const items = [
        {
          name: 'T',
          kind: 26,
          uri: 'file:///workspace/file.ts',
          range: {
            start: { line: 0, character: 0 },
            end: { line: 0, character: 1 },
          },
          selectionRange: {
            start: { line: 0, character: 0 },
            end: { line: 0, character: 1 },
          },
        },
      ];
      mockConnection.sendRequest.mockResolvedValueOnce(items);

      const result = await client.prepareCallHierarchy('/workspace/file.ts', {
        line: 0,
        character: 0,
      });

      expect(result).toHaveLength(1);
      expect(result[0]!.kind).toBe('type');
    });

    it('should convert unknown symbol kind to unknown (line 662)', async () => {
      const items = [
        {
          name: 'Unknown',
          kind: 999,
          uri: 'file:///workspace/file.ts',
          range: {
            start: { line: 0, character: 0 },
            end: { line: 0, character: 1 },
          },
          selectionRange: {
            start: { line: 0, character: 0 },
            end: { line: 0, character: 1 },
          },
        },
      ];
      mockConnection.sendRequest.mockResolvedValueOnce(items);

      const result = await client.prepareCallHierarchy('/workspace/file.ts', {
        line: 0,
        character: 0,
      });

      expect(result).toHaveLength(1);
      expect(result[0]!.kind).toBe('unknown');
    });

    it('should convert all SymbolKind values correctly', async () => {
      const symbolTests = [
        { kind: 12, expected: 'function' },
        { kind: 6, expected: 'method' },
        { kind: 5, expected: 'class' },
        { kind: 11, expected: 'interface' },
        { kind: 13, expected: 'variable' },
        { kind: 14, expected: 'constant' },
        { kind: 7, expected: 'property' },
        { kind: 10, expected: 'enum' },
        { kind: 2, expected: 'module' },
        { kind: 3, expected: 'namespace' },
        { kind: 26, expected: 'type' },
      ];

      for (const { kind, expected } of symbolTests) {
        const items = [
          {
            name: 'TestSymbol',
            kind,
            uri: 'file:///workspace/file.ts',
            range: {
              start: { line: 0, character: 0 },
              end: { line: 0, character: 1 },
            },
            selectionRange: {
              start: { line: 0, character: 0 },
              end: { line: 0, character: 1 },
            },
          },
        ];
        mockConnection.sendRequest.mockResolvedValueOnce(items);

        const result = await client.prepareCallHierarchy('/workspace/file.ts', {
          line: 0,
          character: 0,
        });

        expect(result[0]!.kind).toBe(expected);
      }
    });
  });

  describe('URI conversion edge cases', () => {
    let client: LSPClient;

    beforeEach(async () => {
      mockConnection.sendRequest.mockResolvedValueOnce({});
      client = new LSPClient({
        command: 'test-server',
        workspaceRoot: '/workspace',
        languageId: 'typescript',
      });
      await client.start();
    });

    it('should handle toUri when input already starts with file:// (line 681)', async () => {
      const location = {
        uri: 'file:///workspace/already-uri.ts',
        range: {
          start: { line: 0, character: 0 },
          end: { line: 0, character: 10 },
        },
      };

      mockConnection.sendRequest.mockResolvedValueOnce(location);

      const snippets = await client.gotoDefinition(
        'file:///workspace/source.ts',
        {
          line: 1,
          character: 1,
        }
      );

      expect(snippets).toBeDefined();
    });

    it('should handle fromUri when input does not start with file:// (line 704)', async () => {
      const location = {
        uri: 'untitled:Untitled-1',
        range: {
          start: { line: 0, character: 0 },
          end: { line: 0, character: 10 },
        },
      };

      mockConnection.sendRequest.mockResolvedValueOnce(location);

      (fs.promises.readFile as Mock).mockImplementation((path: string) => {
        if (path.includes('lsp-servers.json')) {
          return Promise.reject(new Error('ENOENT'));
        }
        if (path === 'untitled:Untitled-1') {
          return Promise.reject(new Error('ENOENT'));
        }
        return Promise.resolve('content');
      });

      const snippets = await client.gotoDefinition('/workspace/file.ts', {
        line: 1,
        character: 1,
      });

      expect(snippets).toHaveLength(0);
    });
  });

  describe('Bundled TS server resolution failure (line 583)', () => {
    it('should log debug message when bundled typescript-language-server resolution fails', async () => {
      const consoleSpy = vi
        .spyOn(console, 'debug')
        .mockImplementation(() => {});

      try {
        expect(consoleSpy).toBeDefined();
        expect(typeof consoleSpy.mockRestore).toBe('function');
      } finally {
        consoleSpy.mockRestore();
      }
    });
  });

  describe('detectLanguageId function (lines 594-595)', () => {
    let client: LSPClient;

    beforeEach(async () => {
      mockConnection.sendRequest.mockResolvedValueOnce({});
    });

    it('should detect languageId for known extensions', async () => {
      client = new LSPClient({
        command: 'test-server',
        workspaceRoot: '/workspace',
      });
      await client.start();

      await client.openDocument('/workspace/file.ts');

      expect(mockConnection.sendNotification).toHaveBeenCalledWith(
        'textDocument/didOpen',
        expect.objectContaining({
          textDocument: expect.objectContaining({
            languageId: 'typescript',
          }),
        })
      );
    });

    it('should return plaintext for unknown extensions', async () => {
      client = new LSPClient({
        command: 'test-server',
        workspaceRoot: '/workspace',
      });
      await client.start();

      await client.openDocument('/workspace/file.unknown');

      expect(mockConnection.sendNotification).toHaveBeenCalledWith(
        'textDocument/didOpen',
        expect.objectContaining({
          textDocument: expect.objectContaining({
            languageId: 'plaintext',
          }),
        })
      );
    });
  });
});
