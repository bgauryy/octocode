import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type {
  CancellationToken,
  MessageConnection,
} from 'vscode-jsonrpc/node.js';
import { LSPClient } from '../src/client.js';
import { sendRequestWithCancellationOnTimeout } from '../src/cancellableRequest.js';
import type { CallHierarchyItem, LanguageServerConfig } from '../src/types.js';

const CUSTOM_SERVER_ROOT = fileURLToPath(
  new URL('../benchmark/custom/', import.meta.url)
);
const TEST_ROOT = fileURLToPath(
  new URL('../test-workspaces/', import.meta.url)
);
const tempDirs: string[] = [];

afterEach(async () => {
  while (tempDirs.length > 0) {
    await rm(tempDirs.pop()!, { recursive: true, force: true });
  }
});

function resolveClientReady(client: LSPClient): void {
  (client as unknown as { resolveReady(): void }).resolveReady();
}

function requestOnlyConnection(
  sendRequest: (
    method: string,
    params: unknown,
    token?: CancellationToken
  ) => Promise<unknown>
): MessageConnection {
  return {
    sendRequest: vi.fn(sendRequest),
  } as unknown as MessageConnection;
}

type ClientInternals = {
  initialized: boolean;
  connection: MessageConnection | null;
  process: unknown;
  handleDocumentOpened(): void;
  registerServerInitiatedHandlers(connection: MessageConnection): void;
};

type HandlerConnection = {
  readonly connection: MessageConnection;
  readonly requests: Map<string, (params?: unknown) => unknown>;
  readonly notifications: Map<string, (params?: unknown) => void>;
};

function asInternals(client: LSPClient): ClientInternals {
  return client as unknown as ClientInternals;
}

function handlerConnection(): HandlerConnection {
  const requests = new Map<string, (params?: unknown) => unknown>();
  const notifications = new Map<string, (params?: unknown) => void>();
  const connection = {
    onRequest: vi.fn(
      (method: string, handler: (params?: unknown) => unknown) => {
        requests.set(method, handler);
      }
    ),
    onNotification: vi.fn(
      (method: string, handler: (params?: unknown) => void) => {
        notifications.set(method, handler);
      }
    ),
  } as unknown as MessageConnection;

  return { connection, requests, notifications };
}

function requiredHandler<T>(handlers: Map<string, T>, method: string): T {
  const handler = handlers.get(method);
  if (!handler) throw new Error(`Missing handler: ${method}`);
  return handler;
}

async function tempWorkspace(prefix: string): Promise<string> {
  await mkdir(TEST_ROOT, { recursive: true });
  const root = await mkdtemp(path.join(TEST_ROOT, prefix));
  tempDirs.push(root);
  return root;
}

function customServerConfig(
  initializationOptions?: LanguageServerConfig['initializationOptions']
): LanguageServerConfig {
  return {
    command: process.execPath,
    args: [path.join(CUSTOM_SERVER_ROOT, 'server.mjs')],
    workspaceRoot: CUSTOM_SERVER_ROOT,
    languageId: 'foo',
    ...(initializationOptions && { initializationOptions }),
  };
}

describe('sendRequestWithCancellationOnTimeout', () => {
  it('returns successful responses and clears the timeout', async () => {
    vi.useFakeTimers();
    try {
      const conn = requestOnlyConnection(async () => ({ ok: true }));

      await expect(
        sendRequestWithCancellationOnTimeout(conn, 'textDocument/hover', {}, 50)
      ).resolves.toEqual({ ok: true });
      expect(vi.getTimerCount()).toBe(0);
    } finally {
      vi.useRealTimers();
    }
  });

  it('cancels the token when a request times out', async () => {
    vi.useFakeTimers();
    try {
      let capturedToken: CancellationToken | undefined;
      const conn = requestOnlyConnection(async (_method, _params, token) => {
        capturedToken = token;
        return new Promise(() => undefined);
      });
      const request = sendRequestWithCancellationOnTimeout(
        conn,
        'textDocument/slow',
        {},
        10
      );
      const assertion = expect(request).rejects.toThrow(
        "LSP request 'textDocument/slow' timed out after 10ms"
      );

      await vi.advanceTimersByTimeAsync(10);

      await assertion;
      expect(capturedToken?.isCancellationRequested).toBe(true);
    } finally {
      vi.useRealTimers();
    }
  });
});

describe('LSPClient', () => {
  it('settles readiness quickly when a server emits no progress', async () => {
    vi.useFakeTimers();
    try {
      const client = new LSPClient(customServerConfig());
      const internals = asInternals(client);
      internals.initialized = true;
      internals.handleDocumentOpened();
      let resolved = false;
      const ready = client.waitForReady(1_000).then(() => {
        resolved = true;
      });

      await vi.advanceTimersByTimeAsync(199);
      expect(resolved).toBe(false);
      await vi.advanceTimersByTimeAsync(1);
      await ready;
      expect(resolved).toBe(true);
    } finally {
      vi.useRealTimers();
    }
  });

  it('waits for progress tokens before resolving readiness', async () => {
    vi.useFakeTimers();
    try {
      const client = new LSPClient(customServerConfig());
      const internals = asInternals(client);
      const handlers = handlerConnection();
      internals.registerServerInitiatedHandlers(handlers.connection);
      internals.initialized = true;
      internals.handleDocumentOpened();

      const progress = requiredHandler(handlers.notifications, '$/progress');
      progress({ token: 'index', value: { kind: 'begin' } });

      let resolved = false;
      const ready = client.waitForReady(1_000).then(() => {
        resolved = true;
      });
      await vi.advanceTimersByTimeAsync(200);
      expect(resolved).toBe(false);

      progress({ token: 'index', value: { kind: 'end' } });
      await ready;
      expect(resolved).toBe(true);
    } finally {
      vi.useRealTimers();
    }
  });

  it('forces readiness after the max progress wait', async () => {
    vi.useFakeTimers();
    try {
      const client = new LSPClient(customServerConfig());
      const internals = asInternals(client);
      const handlers = handlerConnection();
      internals.registerServerInitiatedHandlers(handlers.connection);
      internals.initialized = true;
      internals.handleDocumentOpened();

      const progress = requiredHandler(handlers.notifications, '$/progress');
      progress({ token: 'stuck-index', value: { kind: 'begin' } });

      let resolved = false;
      const ready = client.waitForReady(20_000).then(() => {
        resolved = true;
      });

      await vi.advanceTimersByTimeAsync(15_200);
      await ready;
      expect(resolved).toBe(true);
    } finally {
      vi.useRealTimers();
    }
  });

  it('registers safe handlers for server-initiated requests', () => {
    const client = new LSPClient(customServerConfig());
    const handlers = handlerConnection();
    asInternals(client).registerServerInitiatedHandlers(handlers.connection);

    expect(
      requiredHandler(
        handlers.requests,
        'workspace/configuration'
      )({
        items: [{ section: 'x' }, { section: 'y' }],
      })
    ).toEqual([{}, {}]);
    expect(
      requiredHandler(handlers.requests, 'workspace/configuration')({})
    ).toEqual([]);
    expect(
      requiredHandler(handlers.requests, 'workspace/workspaceFolders')()
    ).toMatchObject([
      {
        uri: expect.stringMatching(/^file:\/\//),
        name: path.basename(CUSTOM_SERVER_ROOT),
      },
    ]);
    expect(
      requiredHandler(handlers.requests, 'client/registerCapability')()
    ).toBeNull();
    expect(
      requiredHandler(handlers.requests, 'client/unregisterCapability')()
    ).toBeNull();
    expect(
      requiredHandler(handlers.requests, 'window/workDoneProgress/create')()
    ).toBeNull();
    expect(
      requiredHandler(handlers.notifications, 'window/logMessage')()
    ).toBeUndefined();
    expect(
      requiredHandler(handlers.notifications, 'window/showMessage')()
    ).toBeUndefined();
  });

  it('swallows shutdown and process cleanup failures during stop', async () => {
    const client = new LSPClient(customServerConfig());
    const internals = asInternals(client);
    const connection = {
      sendRequest: vi.fn().mockRejectedValue(new Error('shutdown failed')),
      dispose: vi.fn(() => {
        throw new Error('dispose failed');
      }),
    } as unknown as MessageConnection;
    const childProcess = {
      exitCode: 0,
      signalCode: null,
      stdin: {
        end: vi.fn(() => {
          throw new Error('stdin end failed');
        }),
        destroy: vi.fn(() => {
          throw new Error('stdin destroy failed');
        }),
      },
      stdout: { destroy: vi.fn() },
      stderr: { destroy: vi.fn() },
      kill: vi.fn(() => {
        throw new Error('kill failed');
      }),
      once: vi.fn(),
      unref: vi.fn(),
    };
    internals.connection = connection;
    internals.process = childProcess;
    internals.initialized = true;

    await expect(client.stop()).resolves.toBeUndefined();
    expect(connection.sendRequest).toHaveBeenCalledWith('shutdown');
    expect(connection.dispose).toHaveBeenCalled();
    expect(childProcess.stdin.end).toHaveBeenCalled();
    expect(childProcess.kill).toHaveBeenCalled();
    expect(childProcess.stdin.destroy).toHaveBeenCalled();
    expect(internals.connection).toBeNull();
    expect(internals.initialized).toBe(false);
  });

  it('forces process termination when the server does not close', async () => {
    vi.useFakeTimers();
    try {
      const client = new LSPClient(customServerConfig());
      const internals = asInternals(client);
      const childProcess = {
        exitCode: null,
        signalCode: null,
        stdin: { end: vi.fn(), destroy: vi.fn() },
        stdout: { destroy: vi.fn() },
        stderr: { destroy: vi.fn() },
        kill: vi.fn((signal?: string) => {
          if (signal === 'SIGKILL') {
            throw new Error('sigkill failed');
          }
          return true;
        }),
        once: vi.fn(),
        unref: vi.fn(),
      };
      internals.process = childProcess;
      internals.initialized = true;

      const stopped = client.stop();
      await vi.advanceTimersByTimeAsync(1_000);
      await stopped;

      expect(childProcess.once).toHaveBeenCalledWith(
        'close',
        expect.any(Function)
      );
      expect(childProcess.kill).toHaveBeenCalledWith();
      expect(childProcess.kill).toHaveBeenCalledWith('SIGKILL');
      expect(childProcess.unref).toHaveBeenCalled();
    } finally {
      vi.useRealTimers();
    }
  });

  it('starts a custom stdio server and performs semantic requests', async () => {
    const client = new LSPClient(
      customServerConfig({ fixtureName: 'unit-custom' })
    );
    const filePath = path.join(CUSTOM_SERVER_ROOT, 'demo.foo');
    const position = { line: 0, character: 8 };

    await client.start();
    await expect(client.start()).rejects.toThrow('LSP client already started');
    await client.waitForReady();
    resolveClientReady(client);
    try {
      await client.openDocument(filePath);
      await client.ensureDocumentSynced(filePath, 'define FooSymbol\n');
      await client.closeDocument(filePath);

      expect(await client.gotoDefinition(filePath, position)).toMatchObject([
        {
          uri: filePath,
          displayRange: { startLine: 1, endLine: 1 },
          content: expect.stringContaining('define FooSymbol'),
        },
      ]);
      expect(await client.findReferences(filePath, position)).toHaveLength(1);
      expect(await client.hover(filePath, position)).toMatchObject({
        contents: {
          value: expect.stringContaining('unit-custom'),
        },
      });
      expect(await client.documentSymbols(filePath)).toMatchObject([
        {
          name: 'FooSymbol',
          detail: 'unit-custom',
        },
      ]);
      expect(await client.typeDefinition(filePath, position)).toEqual([]);
      expect(await client.implementation(filePath, position)).toEqual([]);
      expect(await client.prepareCallHierarchy(filePath, position)).toEqual([]);
      expect(await client.getIncomingCalls(callItem(filePath))).toEqual([]);
      expect(await client.getOutgoingCalls(callItem(filePath))).toEqual([]);
      expect(client.hasCapability('definitionProvider')).toBe(true);
      expect(client.hasCapability('workspaceSymbolProvider')).toBe(false);
      expect(client.getRecentStderr()).toEqual([]);
    } finally {
      await client.stop();
    }
  });

  it('surfaces early server startup failure with stderr context', async () => {
    const root = await tempWorkspace('client-fail-');
    const serverPath = path.join(root, 'fail.mjs');
    await writeFile(
      serverPath,
      'process.stderr.write("custom server boom\\n"); process.exit(3);\n'
    );
    const client = new LSPClient({
      command: process.execPath,
      args: [serverPath],
      workspaceRoot: root,
      languageId: 'foo',
    });

    await expect(client.start()).rejects.toThrow(
      /Language server startup failed: .*custom server boom/s
    );
  });

  it('reports no capabilities before initialization', () => {
    const client = new LSPClient(customServerConfig());

    expect(client.hasCapability('definitionProvider')).toBe(false);
  });
});

function callItem(filePath: string): CallHierarchyItem {
  const range = {
    start: { line: 0, character: 7 },
    end: { line: 0, character: 16 },
  };
  return {
    name: 'FooSymbol',
    kind: 'function',
    uri: filePath,
    range,
    selectionRange: range,
    displayRange: { startLine: 1, endLine: 1 },
  };
}
