import { describe, expect, it, vi } from 'vitest';

const mockUpdateSessionStats = vi.hoisted(() => vi.fn());
const mockIncrementRateLimits = vi.hoisted(() => vi.fn());
const mockIncrementGitHubCacheRateLimits = vi.hoisted(() => vi.fn());

vi.mock('../src/shared/session/index.js', () => ({
  updateSessionStats: mockUpdateSessionStats,
  incrementRateLimits: mockIncrementRateLimits,
  incrementGitHubCacheRateLimits: mockIncrementGitHubCacheRateLimits,
}));

const { recordRateLimit, recordPackageRegistryFailure } = await import(
  '../src/session.js'
);

describe('recordRateLimit', () => {
  it('calls updateSessionStats when provider is present', () => {
    mockUpdateSessionStats.mockReturnValueOnce({ session: null });
    recordRateLimit({ provider: 'github' });
    expect(mockUpdateSessionStats).toHaveBeenCalledWith(
      expect.objectContaining({ rateLimits: 1 })
    );
    expect(mockIncrementRateLimits).not.toHaveBeenCalled();
  });

  it('increments github cache rate limits when provider is github and session exists', () => {
    mockUpdateSessionStats.mockReturnValueOnce({ session: {} });
    recordRateLimit({ provider: 'github' });
    expect(mockIncrementGitHubCacheRateLimits).toHaveBeenCalledWith(1);
  });

  it('calls incrementRateLimits when provider is absent', () => {
    mockIncrementRateLimits.mockReturnValueOnce({ session: null });
    recordRateLimit({} as Parameters<typeof recordRateLimit>[0]);
    expect(mockIncrementRateLimits).toHaveBeenCalledWith(1);
    expect(mockUpdateSessionStats).not.toHaveBeenCalled();
  });
});

describe('recordPackageRegistryFailure', () => {
  it('calls updateSessionStats with the registry name', () => {
    recordPackageRegistryFailure('npm');
    expect(mockUpdateSessionStats).toHaveBeenCalledWith(
      expect.objectContaining({
        packageRegistryFailures: { npm: 1 },
      })
    );
  });
});
