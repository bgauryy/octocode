import { mkdir, mkdtemp, realpath, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { describe, expect, it, vi } from 'vitest';

async function withMockedManager(
  run: (mocks: {
    client: { stop: ReturnType<typeof vi.fn> };
    nativeBinding: Record<string, ReturnType<typeof vi.fn>>;
  }) => Promise<void>,
  options: {
    startError?: Error;
    acquireNull?: boolean;
    config?: {
      command: string;
      args?: string[];
      languageId?: string;
    };
  } = {}
) {
  vi.resetModules();
  const client = {
    stop: vi.fn().mockResolvedValue(undefined),
    isAlive: vi.fn().mockResolvedValue(true),
    hasCapability: vi.fn(
      (capability: string) => capability === 'definitionProvider'
    ),
    getReadiness: vi.fn(() => 'progressIdle'),
  };
  const pooledConfigs: unknown[] = [];
  const nativeBinding = {
    configureLspClientPool: vi.fn().mockResolvedValue(undefined),
    acquirePooledLspClient: options.startError
      ? vi.fn().mockRejectedValue(options.startError)
      : vi.fn().mockImplementation(async (config: unknown) => {
          if (options.acquireNull) return null;
          if (pooledConfigs.length === 0) pooledConfigs.push(config);
          return client;
        }),
    releasePooledLspClient: vi.fn().mockImplementation(async () => {
      const removed = pooledConfigs.length > 0;
      pooledConfigs.length = 0;
      return removed;
    }),
    clearPooledLspClients: vi.fn().mockImplementation(async () => {
      pooledConfigs.length = 0;
    }),
    pooledLspClientCount: vi.fn(() => pooledConfigs.length),
    pooledLspClientConfigs: vi.fn(async () => [...pooledConfigs]),
    isCommandAvailable: vi.fn(() => true),
    resolveWorkspaceRootForFile: vi.fn(() => '/workspace'),
  };
  const buildConfig = (filePath: string, workspaceRoot: string) => {
    if (filePath.endsWith('.missing')) return null;
    return {
      command: process.execPath,
      args: [],
      languageId: filePath.endsWith('.py') ? 'python' : 'typescript',
      ...options.config,
      workspaceRoot,
    };
  };
  vi.doMock('../../src/lsp/config.js', () => ({
    resolveServerForFile: vi.fn((filePath: string, workspaceRoot: string) => {
      const config = buildConfig(filePath, workspaceRoot);
      return config ? { config, source: 'path' } : null;
    }),
  }));
  vi.doMock('../../src/lsp/native.js', () => ({ nativeBinding }));
  vi.doMock('../../src/lsp/workspaceRoot.js', () => ({
    resolveWorkspaceRootForFile: vi.fn(() => Promise.resolve('/workspace')),
  }));
  try {
    await run({ client, nativeBinding });
  } finally {
    vi.doUnmock('../../src/lsp/config.js');
    vi.doUnmock('../../src/lsp/native.js');
    vi.doUnmock('../../src/lsp/workspaceRoot.js');
    vi.resetModules();
  }
}

describe('manager wrapper flow', () => {
  it('acquires, caches, reports, and releases native-backed pooled clients', async () => {
    await withMockedManager(async () => {
      const manager = await import('../../src/lsp/manager.js');

      await expect(
        manager.isLanguageServerAvailable('/workspace/a.ts', '/workspace')
      ).resolves.toBe(true);
      const client = await manager.acquirePooledClient(
        '/workspace',
        '/workspace/a.ts'
      );
      expect(client).toBeTruthy();
      expect(manager.pooledClientCount()).toBe(1);
      await expect(
        manager.getLspStatus({ filePath: '/workspace/a.ts' })
      ).resolves.toMatchObject({
        enabled: true,
        workspaceRoot: '/workspace',
        languageId: 'typescript',
        serverAvailable: true,
      });
      await expect(
        manager.releasePooledClientForFile('/workspace', '/workspace/a.ts')
      ).resolves.toBe(true);
      expect(manager.pooledClientCount()).toBe(0);
      await manager.releaseAllPooledClients();
    });
  });

  it('returns unavailable status for files without native server config', async () => {
    await withMockedManager(async () => {
      const manager = await import('../../src/lsp/manager.js');

      await expect(
        manager.acquirePooledClientDetailed(
          '/workspace',
          '/workspace/a.missing'
        )
      ).resolves.toMatchObject({ ok: false, kind: 'unavailable' });
      await expect(
        manager.acquirePooledClient('/workspace', '/workspace/a.missing')
      ).resolves.toBeNull();
      await expect(
        manager.releasePooledClientForFile('/workspace', '/workspace/a.missing')
      ).resolves.toBe(false);
      await expect(
        manager.getLspStatus({ filePath: '/workspace/a.missing' })
      ).resolves.toMatchObject({
        enabled: true,
        languageId: undefined,
        serverAvailable: false,
      });
    });
  });

  it('preserves startup failure details on detailed acquisition', async () => {
    await withMockedManager(
      async () => {
        const manager = await import('../../src/lsp/manager.js');

        await expect(
          manager.acquirePooledClientDetailed('/workspace', '/workspace/a.ts')
        ).resolves.toMatchObject({
          ok: false,
          kind: 'startupFailed',
          message: 'boom from server',
        });
        await expect(
          manager.acquirePooledClient('/workspace', '/workspace/a.ts')
        ).resolves.toBeNull();
      },
      { startError: new Error('boom from server') }
    );
  });

  it('preserves a generic startup failure when the native pool returns no client', async () => {
    await withMockedManager(
      async () => {
        const manager = await import('../../src/lsp/manager.js');

        await expect(
          manager.acquirePooledClientDetailed('/workspace', '/workspace/a.ts')
        ).resolves.toMatchObject({
          ok: false,
          kind: 'startupFailed',
          message: 'Language server failed to start.',
        });
      },
      { acquireNull: true }
    );
  });

  it('returns a resolved-server receipt with invocation, provenance, and semantic state', async () => {
    await withMockedManager(async () => {
      const manager = await import('../../src/lsp/manager.js');

      const result = await manager.acquirePooledClientDetailed(
        '/workspace',
        '/workspace/a.ts'
      );

      expect(result).toMatchObject({
        ok: true,
        receipt: {
          command: process.execPath,
          argv: [],
          source: 'path',
          workspaceRoot: '/workspace',
          configurationFingerprint: expect.any(String),
          workspaceFingerprint: expect.any(String),
          readiness: 'progressIdle',
          capabilities: {
            definitionProvider: true,
            referencesProvider: false,
          },
          identity: {
            artifacts: [
              expect.objectContaining({
                role: 'command',
                path: process.execPath,
                size: expect.any(Number),
              }),
            ],
          },
        },
      });
    });
  });

  it('omits missing absolute server artifacts from the provenance receipt', async () => {
    const root = await mkdtemp(path.join(os.tmpdir(), 'octocode-lsp-missing-'));
    const command = path.join(root, 'missing-server');
    try {
      await withMockedManager(
        async () => {
          const manager = await import('../../src/lsp/manager.js');
          const result = await manager.acquirePooledClientDetailed(
            root,
            path.join(root, 'a.ts')
          );

          expect(result).toMatchObject({
            ok: true,
            receipt: {
              command,
              identity: { artifacts: [], packages: [] },
            },
          });
        },
        { config: { command } }
      );
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });

  it('records package identity for resolved server artifacts under node_modules', async () => {
    const root = await mkdtemp(path.join(os.tmpdir(), 'octocode-lsp-receipt-'));
    const packageRoot = path.join(root, 'node_modules', '@fixture', 'server');
    const command = path.join(packageRoot, 'bin.js');
    try {
      await mkdir(packageRoot, { recursive: true });
      await writeFile(command, '#!/usr/bin/env node\n');
      await writeFile(
        path.join(packageRoot, 'package.json'),
        JSON.stringify({ name: '@fixture/server', version: '1.2.3' })
      );
      const canonicalCommand = await realpath(command);
      const canonicalPackageRoot = path.dirname(canonicalCommand);

      await withMockedManager(
        async () => {
          const manager = await import('../../src/lsp/manager.js');
          const result = await manager.acquirePooledClientDetailed(
            root,
            path.join(root, 'a.ts')
          );

          expect(result).toMatchObject({
            ok: true,
            receipt: {
              command,
              identity: {
                artifacts: [
                  expect.objectContaining({
                    role: 'command',
                    path: canonicalCommand,
                    sha256: expect.any(String),
                  }),
                ],
                packages: [
                  expect.objectContaining({
                    name: '@fixture/server',
                    version: '1.2.3',
                    manifestPath: path.join(
                      canonicalPackageRoot,
                      'package.json'
                    ),
                    manifestSha256: expect.any(String),
                  }),
                ],
              },
            },
          });
        },
        { config: { command } }
      );
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });

  it('parses invalid pool idle timeout env values back to default', async () => {
    await withMockedManager(async () => {
      const manager = await import('../../src/lsp/manager.js');

      expect(manager.parsePoolIdleTimeoutMs('garbage')).toBe(60_000);
      expect(manager.parsePoolIdleTimeoutMs('0')).toBe(60_000);
      expect(manager.parsePoolIdleTimeoutMs('2500')).toBe(2_500);
    });
  });

  it('parses and bounds the LSP pool capacity', async () => {
    await withMockedManager(async () => {
      const manager = await import('../../src/lsp/manager.js');

      expect(manager.parsePoolMaxEntries('garbage')).toBe(4);
      expect(manager.parsePoolMaxEntries('0')).toBe(4);
      expect(manager.parsePoolMaxEntries('8')).toBe(8);
      expect(manager.parsePoolMaxEntries('100')).toBe(32);
    });
  });
});
