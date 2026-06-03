/**
 * Race Condition Tests for Dynamic Token Resolution
 *
 * Tests that token resolution is truly dynamic with no caching,
 * handling concurrent requests and runtime changes correctly.
 *
 * Uses resolveTokenFull mock to simulate various token states.
 */

import { describe, it, expect, vi, beforeEach, afterEach, Mock } from 'vitest';
import {
  getGitHubToken,
  cleanup,
  _setTokenResolvers,
  _resetTokenResolvers,
} from '../src/serverConfig.js';
import type { FullTokenResolution } from 'octocode-shared';

// Type for resolveTokenFull mock
type ResolveTokenFullMock = Mock<
  (options?: {
    hostname?: string;
    clientId?: string;
    getGhCliToken?: (
      hostname?: string
    ) => string | null | Promise<string | null>;
  }) => Promise<FullTokenResolution | null>
>;

let mockResolveTokenFull: ResolveTokenFullMock;

// Helper to create token resolution result
function mockTokenResult(
  token: string | null,
  source:
    | 'env:OCTOCODE_TOKEN'
    | 'env:GH_TOKEN'
    | 'env:GITHUB_TOKEN'
    | 'octocode-storage'
    | 'gh-cli'
    | null
): FullTokenResolution | null {
  if (!token) return null;
  return {
    token,
    source,
    wasRefreshed: false,
  };
}

// Helper to setup resolveTokenFull mock
function setupTokenMocks() {
  mockResolveTokenFull = vi.fn(async () => null);

  _setTokenResolvers({
    resolveTokenFull: mockResolveTokenFull,
  });
}

describe('ServerConfig Dynamic Token Resolution', () => {
  const originalEnv = process.env;

  beforeEach(() => {
    vi.clearAllMocks();
    cleanup();

    // Reset environment variables
    process.env = { ...originalEnv };
    delete process.env.GITHUB_TOKEN;
    delete process.env.GH_TOKEN;
    delete process.env.OCTOCODE_TOKEN;

    // Setup mocks
    setupTokenMocks();
  });

  afterEach(() => {
    process.env = originalEnv;
    cleanup();
    _resetTokenResolvers();
  });

  it('should resolve token fresh on each call (no caching)', async () => {
    let callCount = 0;

    // Mock that returns different token each call
    mockResolveTokenFull.mockImplementation(async () => {
      callCount++;
      return mockTokenResult(`token-${callCount}`, 'gh-cli');
    });

    // Each call should trigger a fresh resolution
    const token1 = await getGitHubToken();
    const token2 = await getGitHubToken();
    const token3 = await getGitHubToken();

    expect(token1).toBe('token-1');
    expect(token2).toBe('token-2');
    expect(token3).toBe('token-3');

    // Each call should trigger resolution (no caching)
    expect(callCount).toBe(3);
  });

  it('should handle concurrent requests independently', async () => {
    let resolveCount = 0;

    // Mock with async delay to simulate real-world behavior
    mockResolveTokenFull.mockImplementation(async () => {
      resolveCount++;
      await new Promise(resolve => setTimeout(resolve, 50));
      return mockTokenResult(`cli-token-${resolveCount}`, 'gh-cli');
    });

    // Start 3 concurrent requests for the token
    const promise1 = getGitHubToken();
    const promise2 = getGitHubToken();
    const promise3 = getGitHubToken();

    const [token1, token2, token3] = await Promise.all([
      promise1,
      promise2,
      promise3,
    ]);

    // Each should get a token (order may vary due to async)
    expect(token1).toMatch(/^cli-token-\d$/);
    expect(token2).toMatch(/^cli-token-\d$/);
    expect(token3).toMatch(/^cli-token-\d$/);

    // All 3 should have triggered resolution (no caching)
    expect(resolveCount).toBe(3);
  });

  it('should pick up token changes immediately', async () => {
    // First call - CLI token
    mockResolveTokenFull.mockResolvedValueOnce(
      mockTokenResult('initial-token', 'gh-cli')
    );
    const token1 = await getGitHubToken();
    expect(token1).toBe('initial-token');

    // Token source changes to storage
    mockResolveTokenFull.mockResolvedValueOnce(
      mockTokenResult('fallback-token', 'octocode-storage')
    );
    const token2 = await getGitHubToken();
    expect(token2).toBe('fallback-token');

    // Token source changes to env
    mockResolveTokenFull.mockResolvedValueOnce(
      mockTokenResult('new-env-token', 'env:GITHUB_TOKEN')
    );
    const token3 = await getGitHubToken();
    expect(token3).toBe('new-env-token');
  });

  it('should handle token deletion at runtime', async () => {
    // Token initially available
    mockResolveTokenFull.mockResolvedValueOnce(
      mockTokenResult('available-token', 'gh-cli')
    );
    const token1 = await getGitHubToken();
    expect(token1).toBe('available-token');

    // Token is deleted (user logged out, token expired, etc.)
    mockResolveTokenFull.mockResolvedValueOnce(null);
    const token2 = await getGitHubToken();
    expect(token2).toBeNull();

    // User logs in again
    mockResolveTokenFull.mockResolvedValueOnce(
      mockTokenResult('new-token', 'gh-cli')
    );
    const token3 = await getGitHubToken();
    expect(token3).toBe('new-token');
  });

  it('should respect token source priority on each call', async () => {
    // Initially CLI token available
    mockResolveTokenFull.mockResolvedValueOnce(
      mockTokenResult('cli-token', 'gh-cli')
    );
    const token1 = await getGitHubToken();
    expect(token1).toBe('cli-token');

    // User sets env var (higher priority - resolveTokenFull returns env token)
    mockResolveTokenFull.mockResolvedValueOnce(
      mockTokenResult('env-token', 'env:GITHUB_TOKEN')
    );
    const token2 = await getGitHubToken();
    expect(token2).toBe('env-token');

    // User removes env var - back to CLI
    mockResolveTokenFull.mockResolvedValueOnce(
      mockTokenResult('cli-token', 'gh-cli')
    );
    const token3 = await getGitHubToken();
    expect(token3).toBe('cli-token');
  });
});
