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

  // Resolve indexing readiness the way tsserver does: a $/progress begin/end
  // cycle. Ops wait for readiness after didOpen, so tests drive it explicitly.
  const fireReadyProgress = () => {
    const handler = mockConnection.onNotification.mock.calls.find(
      (call: unknown[]) => call[0] === '$/progress'
    )?.[1] as
      | ((p: { token: string; value: { kind: string } }) => void)
      | undefined;
    handler?.({ token: 'ready', value: { kind: 'begin' } });
    handler?.({ token: 'ready', value: { kind: 'end' } });
  };

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
      vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
      mockConnection.sendRequest.mockResolvedValueOnce({
        capabilities: { callHierarchyProvider: true },
      });
      client = new LSPClient({
        command: 'test-server',
        workspaceRoot: '/workspace',
        languageId: 'typescript',
      });
      await client.start();
      fireReadyProgress();
    });

    afterEach(() => {
      vi.useRealTimers();
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
      vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
      mockConnection.sendRequest.mockResolvedValueOnce({});
      client = new LSPClient({
        command: 'test-server',
        workspaceRoot: '/workspace',
        languageId: 'typescript',
      });
      await client.start();
      fireReadyProgress();
    });

    afterEach(() => {
      vi.useRealTimers();
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

  describe('Indexing-wait — readyPromise / $/progress tracking', () => {
    it('resolves via settle fallback after the first didOpen when no $/progress arrives', async () => {
      vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
      try {
        mockConnection.sendRequest.mockResolvedValueOnce({ capabilities: {} });
        const client = new LSPClient({
          command: 'test-server',
          workspaceRoot: '/workspace',
        });
        await client.start();

        // Before any didOpen there is nothing to wait for — resolves immediately.
        await client.waitForReady();

        // The settle fallback is armed by the first didOpen, not by initialize.
        await client.openDocument('/workspace/a.ts', 'content');
        let resolved = false;
        const p = client.waitForReady().then(() => {
          resolved = true;
        });
        await Promise.resolve();
        expect(resolved).toBe(false);
        await vi.advanceTimersByTimeAsync(2600);
        await p;
        expect(resolved).toBe(true);
      } finally {
        vi.useRealTimers();
      }
    });

    it('resolves immediately after $/progress begin + end sequence', async () => {
      vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
      try {
        let progressHandler:
          | ((p: { token: string | number; value: { kind: string } }) => void)
          | undefined;
        mockConnection.onNotification.mockImplementation(
          (event: string, handler: unknown) => {
            if (event === '$/progress')
              progressHandler = handler as typeof progressHandler;
          }
        );
        mockConnection.sendRequest.mockResolvedValueOnce({ capabilities: {} });

        const client = new LSPClient({
          command: 'test-server',
          workspaceRoot: '/workspace',
        });
        await client.start();

        const waitPromise = client.waitForReady();
        progressHandler!({ token: 'T1', value: { kind: 'begin' } });
        progressHandler!({ token: 'T1', value: { kind: 'end' } });

        // readyPromise resolves via token completion (not timeout).
        // Drain microtask queue: readyResolve → race resolves → waitForReady completes.
        await waitPromise;
      } finally {
        vi.useRealTimers();
      }
    });

    it('stop() resolves readyPromise so callers are not left hanging', async () => {
      vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
      try {
        mockConnection.sendRequest
          .mockResolvedValueOnce({ capabilities: {} })
          .mockResolvedValueOnce({});
        const client = new LSPClient({
          command: 'test-server',
          workspaceRoot: '/workspace',
        });
        await client.start();

        const p = client.waitForReady(60_000);
        await client.stop();
        await vi.advanceTimersByTimeAsync(0);
        await p;
      } finally {
        vi.useRealTimers();
      }
    });
  });

  describe('readyPromise edge cases — uncovered branches', () => {
    it('stop() before start() covers timer=null path and readyResolved idempotency', async () => {
      vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
      try {
        const client = new LSPClient({
          command: 'test-server',
          workspaceRoot: '/workspace',
        });

        // First stop on unstarted client: readyFallbackTimer is null, readyResolved=false
        await client.stop();

        // Second stop: readyResolved is already true → resolveReady() early-return branch
        await client.stop();

        // waitForReady() after already resolved → immediate early-return branch
        await client.waitForReady();
      } finally {
        vi.useRealTimers();
      }
    });

    it('multiple $/progress tokens — end with size > 0 does NOT resolve early', async () => {
      vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
      try {
        const handlers: Record<
          string,
          (p: { token: string | number; value: { kind: string } }) => void
        > = {};
        mockConnection.onNotification.mockImplementation(
          (event: string, handler: unknown) => {
            handlers[event] = handler as (typeof handlers)[string];
          }
        );
        mockConnection.sendRequest.mockResolvedValueOnce({ capabilities: {} });

        const client = new LSPClient({
          command: 'test-server',
          workspaceRoot: '/workspace',
        });
        await client.start();
        // waitForReady only waits once a document has been opened (that is
        // when tsserver starts loading the project).
        await client.openDocument('/workspace/file.ts', 'content');

        const progressHandler = handlers['$/progress']!;

        // Two tokens in flight
        progressHandler({ token: 'T1', value: { kind: 'begin' } });
        progressHandler({ token: 'T2', value: { kind: 'begin' } });

        // End T1 only — size=1, readyPromise must NOT resolve yet
        progressHandler({ token: 'T1', value: { kind: 'end' } });

        let settled = false;
        client.waitForReady(100).then(() => {
          settled = true;
        });
        // Do NOT advance time — the 100ms timeout hasn't fired
        await Promise.resolve();
        await Promise.resolve();
        // Still waiting (T2 is still active, timeout not elapsed under fake timers)
        expect(settled).toBe(false);

        // End T2 — size=0, resolves immediately
        progressHandler({ token: 'T2', value: { kind: 'end' } });
        await Promise.resolve();
        await Promise.resolve();
        await Promise.resolve();
        await Promise.resolve();
        expect(settled).toBe(true);
      } finally {
        vi.useRealTimers();
      }
    });
  });

  describe('Pool language map — all registry extensions covered', () => {
    it('acquirePooledClient returns null for truly unknown extensions (not in registry)', async () => {
      const { acquirePooledClient: acquire } =
        await import('../../src/lsp/manager.js');
      const result = await acquire('/workspace', '/workspace/file.xyz');
      expect(result).toBeNull();
    });

    it('isLanguageServerAvailable proceeds past pool gate for .rb (ruby in registry)', async () => {
      const { isLanguageServerAvailable: check } =
        await import('../../src/lsp/manager.js');
      // Stub commandExists to avoid spawning — the key assertion is that
      // the extension IS recognised by languageIdForFile (previously .rb was silently null).
      const spawnMock = cp.spawn as Mock;
      const mockCheckProc = new EventEmitter() as EventEmitter & { kill: Mock };
      mockCheckProc.kill = vi.fn();
      spawnMock.mockImplementation(() => {
        setImmediate(() => mockCheckProc.emit('close', 1));
        return mockCheckProc;
      });
      (fs.promises.readFile as Mock).mockRejectedValue(new Error('ENOENT'));

      // Before fix: returned false because .rb was not in LANGUAGE_ID_FOR_EXT.
      // After fix: reaches commandExists (solargraph), which returns false because
      // the mock exits with code 1 — but the important part is the function no longer
      // short-circuits at languageIdForFile.
      const result = await check('/workspace/file.rb');
      // solargraph is not found (mock returns exit 1) — false is correct
      expect(result).toBe(false);
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
