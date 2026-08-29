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
});
