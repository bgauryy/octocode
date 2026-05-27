import { describe, expect, it, vi, beforeEach } from 'vitest';
import { PROVIDER_CAPABILITIES } from '../../src/providers/capabilities.js';

const mockGetProvider = vi.hoisted(() => vi.fn());
const mockGetActiveProvider = vi.hoisted(() => vi.fn(() => 'github'));
const mockGetActiveProviderConfig = vi.hoisted(() =>
  vi.fn(() => ({
    provider: 'github',
    baseUrl: undefined,
    token: 'mock-token',
  }))
);

vi.mock('../../src/providers/factory.js', () => ({
  getProvider: mockGetProvider,
}));

vi.mock('../../src/serverConfig.js', () => ({
  getActiveProvider: mockGetActiveProvider,
  getActiveProviderConfig: mockGetActiveProviderConfig,
}));

import {
  createProviderExecutionContext,
  executeProviderOperation,
  executeProviderOperations,
  ProviderInitializationError,
} from '../../src/tools/providerExecution.js';

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

  it('should partition multi-operation results without normalizing failures', async () => {
    const result = await executeProviderOperations([
      {
        meta: { label: 'topics' },
        operation: async () => ({
          data: {
            repositories: [],
            totalCount: 0,
            pagination: {
              currentPage: 1,
              totalPages: 0,
              hasMore: false,
            },
          },
          status: 200,
          provider: 'github',
        }),
      },
      {
        meta: { label: 'keywords' },
        operation: async () => ({
          error: 'rate limited',
          status: 429,
          provider: 'github',
        }),
      },
    ]);

    expect(result.successes).toHaveLength(1);
    expect(result.failures).toHaveLength(1);
    expect(result.successes[0]?.meta.label).toBe('topics');
    expect(result.failures[0]?.response.error).toBe('rate limited');
  });

  it('should preserve partial successes when one provider operation throws', async () => {
    const result = await executeProviderOperations([
      {
        meta: { label: 'topics' },
        operation: async () => ({
          data: {
            repositories: [],
            totalCount: 0,
            pagination: {
              currentPage: 1,
              totalPages: 0,
              hasMore: false,
            },
          },
          status: 200,
          provider: 'github',
        }),
      },
      {
        meta: { label: 'keywords' },
        operation: async () => {
          throw new Error('network exploded');
        },
      },
    ]);

    expect(result.successes).toHaveLength(1);
    expect(result.failures).toHaveLength(1);
    expect(result.successes[0]?.meta.label).toBe('topics');
    expect(result.failures[0]?.meta.label).toBe('keywords');
    expect(result.failures[0]?.response.error).toBe('network exploded');
    expect(result.failures[0]?.response.status).toBe(500);
    expect(result.failures[0]?.response.provider).toBe('github');
  });

  it('should use passed providerType in error response when operation throws', async () => {
    mockGetActiveProviderConfig.mockReturnValue({
      provider: 'github',
      baseUrl: undefined,
      token: 'mock-token',
    });

    const result = await executeProviderOperations(
      [
        {
          meta: { label: 'op' },
          operation: async () => {
            throw new Error('github api down');
          },
        },
      ],
      'github'
    );

    expect(result.failures).toHaveLength(1);
    expect(result.failures[0]?.response.provider).toBe('github');
    expect(result.failures[0]?.response.error).toBe('github api down');
  });
});
