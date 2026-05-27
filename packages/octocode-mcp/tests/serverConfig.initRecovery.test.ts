import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mockGetConfigSync = vi.hoisted(() => vi.fn());
const mockInvalidateConfigCache = vi.hoisted(() => vi.fn());

vi.mock('octocode-shared', async () => {
  const actual =
    await vi.importActual<typeof import('octocode-shared')>('octocode-shared');

  return {
    ...actual,
    getConfigSync: mockGetConfigSync,
    invalidateConfigCache: mockInvalidateConfigCache,
  };
});

describe('serverConfig initialize recovery', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.resetModules();
  });

  afterEach(async () => {
    const serverConfig = await import('../src/serverConfig.js');
    serverConfig.cleanup();
    serverConfig._resetTokenResolvers();
  });

  it('should allow a second initialize attempt after the first one fails', async () => {
    mockGetConfigSync
      .mockImplementationOnce(() => {
        throw new Error('broken config file');
      })
      .mockReturnValue({
        github: { apiUrl: 'https://api.github.com' },
        tools: {
          enabled: undefined,
          enableAdditional: undefined,
          disabled: undefined,
        },
        network: { timeout: 30000, maxRetries: 3 },
        telemetry: { logging: true },
        local: { enabled: false, enableClone: false },
        output: { format: 'yaml' },
      });

    const serverConfig = await import('../src/serverConfig.js');
    serverConfig._setTokenResolvers({
      resolveTokenFull: vi.fn().mockResolvedValue(null),
    });

    await expect(serverConfig.initialize()).rejects.toThrow(
      'broken config file'
    );
    await expect(serverConfig.initialize()).resolves.toBeUndefined();

    expect(serverConfig.getServerConfig()).toMatchObject({
      githubApiUrl: 'https://api.github.com',
      timeout: 30000,
      maxRetries: 3,
      tokenSource: 'none',
    });
    expect(mockGetConfigSync.mock.calls.length).toBeGreaterThan(1);
  });
});
