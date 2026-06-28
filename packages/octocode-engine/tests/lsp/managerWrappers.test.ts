import { describe, expect, it, vi } from 'vitest';

async function withMockedManager(run: () => Promise<void>) {
  vi.resetModules();
  const client = {
    start: vi.fn().mockResolvedValue(undefined),
    stop: vi.fn().mockResolvedValue(undefined),
  };
  const LSPClient = vi.fn(function LSPClient() {
    return client;
  });
  vi.doMock('../../src/lsp/client.js', () => ({ LSPClient }));
  const buildConfig = (filePath: string, workspaceRoot: string) => {
    if (filePath.endsWith('.missing')) return null;
    return {
      command: process.execPath,
      args: [],
      workspaceRoot,
      languageId: filePath.endsWith('.py') ? 'python' : 'typescript',
    };
  };
  vi.doMock('../../src/lsp/config.js', () => ({
    getLanguageServerForFile: vi.fn(buildConfig),
    resolveServerForFile: vi.fn(
      (filePath: string, workspaceRoot: string) => {
        const config = buildConfig(filePath, workspaceRoot);
        return config ? { config, source: 'path' } : null;
      }
    ),
  }));
  vi.doMock('../../src/lsp/native.js', () => ({
    nativeBinding: {
      isCommandAvailable: vi.fn(() => true),
      resolveWorkspaceRootForFile: vi.fn(() => '/workspace'),
    },
  }));
  vi.doMock('../../src/lsp/workspaceRoot.js', () => ({
    resolveWorkspaceRootForFile: vi.fn(() => Promise.resolve('/workspace')),
  }));
  try {
    await run();
  } finally {
    vi.doUnmock('../../src/lsp/client.js');
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
});
