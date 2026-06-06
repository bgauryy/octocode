import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { withRetry, RETRY_CONFIGS } from '../../utils/retry.js';

describe('withRetry', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('returns immediately on success', async () => {
    const operation = vi.fn().mockResolvedValue('success');

    const resultPromise = withRetry(operation, RETRY_CONFIGS.local);
    const result = await resultPromise;

    expect(result).toBe('success');
    expect(operation).toHaveBeenCalledTimes(1);
  });

  it('retries on retryable error', async () => {
    const operation = vi
      .fn()
      .mockRejectedValueOnce({ code: 'EBUSY' })
      .mockResolvedValue('success');

    const resultPromise = withRetry(operation, RETRY_CONFIGS.local);

    await vi.advanceTimersByTimeAsync(0);

    await vi.advanceTimersByTimeAsync(100);

    const result = await resultPromise;

    expect(result).toBe('success');
    expect(operation).toHaveBeenCalledTimes(2);
  });

  it('throws on non-retryable error', async () => {
    const operation = vi.fn().mockRejectedValue({ code: 'ENOENT' });

    await expect(withRetry(operation, RETRY_CONFIGS.local)).rejects.toEqual({
      code: 'ENOENT',
    });
    expect(operation).toHaveBeenCalledTimes(1);
  });

  it('applies exponential backoff', async () => {
    const operation = vi
      .fn()
      .mockRejectedValueOnce({ code: 'ETIMEDOUT' })
      .mockRejectedValueOnce({ code: 'ETIMEDOUT' })
      .mockResolvedValue('success');

    const config = {
      maxAttempts: 3,
      initialDelayMs: 100,
      maxDelayMs: 1000,
      backoffMultiplier: 2,
      retryOn: (err: unknown) => (err as { code?: string })?.code === 'ETIMEDOUT',
    };

    const resultPromise = withRetry(operation, config);

    await vi.advanceTimersByTimeAsync(0);

    await vi.advanceTimersByTimeAsync(100);
    expect(operation).toHaveBeenCalledTimes(2);

    await vi.advanceTimersByTimeAsync(200);

    const result = await resultPromise;
    expect(result).toBe('success');
    expect(operation).toHaveBeenCalledTimes(3);
  });

  it('caps delay at maxDelayMs', async () => {
    const operation = vi
      .fn()
      .mockRejectedValueOnce({ code: 'ETIMEDOUT' })
      .mockRejectedValueOnce({ code: 'ETIMEDOUT' })
      .mockResolvedValue('success');

    const config = {
      maxAttempts: 3,
      initialDelayMs: 500,
      maxDelayMs: 600,
      backoffMultiplier: 2,
      retryOn: (err: unknown) => (err as { code?: string })?.code === 'ETIMEDOUT',
    };

    const resultPromise = withRetry(operation, config);

    await vi.advanceTimersByTimeAsync(0);

    await vi.advanceTimersByTimeAsync(500);

    await vi.advanceTimersByTimeAsync(600);

    const result = await resultPromise;
    expect(result).toBe('success');
    expect(operation).toHaveBeenCalledTimes(3);
  });

  it('throws last error after all retries exhausted', async () => {
    const finalError = new Error('Operation failed');
    const operation = vi.fn().mockRejectedValue(finalError);

    const config = {
      maxAttempts: 2,
      initialDelayMs: 50,
      maxDelayMs: 100,
      backoffMultiplier: 2,
      retryOn: () => true,
    };

    const resultPromise = withRetry(operation, config).catch((e) => e);

    await vi.runAllTimersAsync();

    const result = await resultPromise;
    expect(result).toBe(finalError);
    expect(operation).toHaveBeenCalledTimes(2);
  });
});

describe('RETRY_CONFIGS', () => {
  describe('lsp config', () => {
    it('retries on LSP not ready errors', () => {
      const config = RETRY_CONFIGS.lsp;

      expect(config.retryOn({ code: 'LSP_NOT_READY' })).toBe(true);
      expect(config.retryOn({ message: 'server not started' })).toBe(true);
      expect(config.retryOn({ message: 'not initialized' })).toBe(true);
      expect(config.retryOn({ code: 'ECONNREFUSED' })).toBe(true);
      expect(config.retryOn({ code: 'ETIMEDOUT' })).toBe(true);
    });

    it('does not retry on unrelated errors', () => {
      const config = RETRY_CONFIGS.lsp;

      expect(config.retryOn({ code: 'ENOENT' })).toBe(false);
      expect(config.retryOn({ message: 'Symbol not found' })).toBe(false);
    });
  });

  describe('github config', () => {
    it('retries on rate limit errors', () => {
      const config = RETRY_CONFIGS.github;

      expect(config.retryOn({ status: 429 })).toBe(true);
      expect(config.retryOn({ status: 403 })).toBe(true);
      expect(config.retryOn({ message: 'rate limit exceeded' })).toBe(true);
    });

    it('retries on server errors', () => {
      const config = RETRY_CONFIGS.github;

      expect(config.retryOn({ status: 500 })).toBe(true);
      expect(config.retryOn({ status: 502 })).toBe(true);
      expect(config.retryOn({ status: 503 })).toBe(true);
    });

    it('does not retry on client errors', () => {
      const config = RETRY_CONFIGS.github;

      expect(config.retryOn({ status: 400 })).toBe(false);
      expect(config.retryOn({ status: 404 })).toBe(false);
    });
  });

  describe('local config', () => {
    it('retries on file busy errors', () => {
      const config = RETRY_CONFIGS.local;

      expect(config.retryOn({ code: 'EBUSY' })).toBe(true);
      expect(config.retryOn({ code: 'EAGAIN' })).toBe(true);
    });

    it('retries on timeout', () => {
      const config = RETRY_CONFIGS.local;

      expect(config.retryOn({ code: 'ETIMEDOUT' })).toBe(true);
    });
  });
});
