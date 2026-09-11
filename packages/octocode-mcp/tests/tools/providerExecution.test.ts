import { describe, expect, it, vi, beforeEach } from 'vitest';
import { PROVIDER_CAPABILITIES } from '../../../octocode-tools-core/src/providers/capabilities.js';

const mockGetProvider = vi.hoisted(() => vi.fn());
const mockGetActiveProvider = vi.hoisted(() => vi.fn(() => 'github'));
const mockGetActiveProviderConfig = vi.hoisted(() =>
  vi.fn(() => ({
    provider: 'github',
    baseUrl: undefined,
    token: 'mock-token',
  }))
);

vi.mock('../../../octocode-tools-core/src/providers/factory.js', () => ({
  getProvider: mockGetProvider,
}));

vi.mock('../../../octocode-tools-core/src/serverConfig.js', () => ({
  getActiveProvider: mockGetActiveProvider,
  getActiveProviderConfig: mockGetActiveProviderConfig,
}));

import {
  createProviderExecutionContext,
  executeProviderOperation,
  ProviderInitializationError,
} from '../../../octocode-tools-core/src/tools/providerExecution.js';

describe('providerExecution', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('should create provider context with provider capabilities', () => {
    mockGetProvider.mockReturnValue({
      capabilities: PROVIDER_CAPABILITIES.github,
    });

    const context = createProviderExecutionContext();

    expect(context.providerType).toBe('github');
    expect(context.capabilities).toBe(PROVIDER_CAPABILITIES.github);
  });

  it('should throw a typed error when provider initialization fails', () => {
    mockGetProvider.mockImplementation(() => {
      throw new Error('unregistered');
    });

    expect(() => createProviderExecutionContext()).toThrowError(
      ProviderInitializationError
    );
    expect(() => createProviderExecutionContext()).toThrow(
      'Failed to initialize github provider: unregistered'
    );
  });

  it('should return provider errors as processed bulk results', async () => {
    const result = await executeProviderOperation(
      { mainResearchGoal: 'test', researchGoal: 'test', reasoning: 'test' },
      async () => ({
        error: 'rate limited',
        status: 429,
        provider: 'github',
        rateLimit: {
          remaining: 0,
          reset: Math.floor(Date.now() / 1000) + 60,
          retryAfter: 60,
        },
      })
    );

    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.result.status).toBe('error');
      expect(JSON.stringify(result.result)).toContain('rate limited');
    }
  });
});
