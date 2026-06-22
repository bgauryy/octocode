import {
  incrementGitHubCacheRateLimits,
  incrementRateLimits,
  updateSessionStats,
} from './shared/index.js';
import type { RateLimitData } from './types/session.js';

export function recordRateLimit(data: RateLimitData): void {
  const result = data.provider
    ? updateSessionStats({
        rateLimits: 1,
        rateLimitsByProvider: {
          [data.provider]: 1,
        },
      } as Parameters<typeof updateSessionStats>[0])
    : incrementRateLimits(1);

  if (result.session && data.provider === 'github') {
    incrementGitHubCacheRateLimits(1);
  }
}

export function recordPackageRegistryFailure(registry: string): void {
  updateSessionStats({
    packageRegistryFailures: {
      [registry]: 1,
    },
  } as Parameters<typeof updateSessionStats>[0]);
}
