import { afterEach, describe, expect, it, vi } from 'vitest';

const { runCacheMaintenanceIfDue } = vi.hoisted(() => ({
  runCacheMaintenanceIfDue: vi.fn().mockResolvedValue(false),
}));

vi.mock('../src/cacheMaintenance.js', () => ({
  runCacheMaintenanceIfDue,
}));

describe('server runtime cache bootstrap', () => {
  afterEach(async () => {
    const serverConfig = await import('../src/serverConfig.js');
    serverConfig.cleanup();
    vi.clearAllMocks();
  });

  it('performs the persisted due-check during shared CLI/MCP initialization', async () => {
    const serverConfig = await import('../src/serverConfig.js');
    serverConfig._setTokenResolvers({
      resolveTokenFull: async () => ({ token: null, source: 'none' }),
    });

    await serverConfig.initialize();

    expect(runCacheMaintenanceIfDue).toHaveBeenCalledOnce();
    expect(runCacheMaintenanceIfDue).toHaveBeenCalledWith(expect.any(String));
  });

  it('does not advertise clone support in memory-only mode', async () => {
    const previous = process.env.OCTOCODE_STORAGE_MODE;
    process.env.OCTOCODE_STORAGE_MODE = 'memory';
    const serverConfig = await import('../src/serverConfig.js');
    serverConfig.cleanup();
    serverConfig._setTokenResolvers({
      resolveTokenFull: async () => ({ token: null, source: 'none' }),
    });

    try {
      await serverConfig.initialize();
      expect(serverConfig.isCloneEnabled()).toBe(false);
    } finally {
      if (previous === undefined) delete process.env.OCTOCODE_STORAGE_MODE;
      else process.env.OCTOCODE_STORAGE_MODE = previous;
    }
  });
});
