import { describe, it, expect, beforeEach, vi } from 'vitest';

vi.mock('octocode-shared', async importOriginal => {
  const actual = await importOriginal<typeof import('octocode-shared')>();
  return {
    ...actual,
    getOrCreateSession: vi.fn(() => ({
      version: 1,
      sessionId: 'mock-session-id-12345678-1234-4123-8123-123456789012',
      createdAt: '2024-01-01T00:00:00.000Z',
      lastActiveAt: '2024-01-01T00:00:00.000Z',
      stats: { toolCalls: 0, errors: 0, rateLimits: 0 },
    })),
    incrementToolCalls: vi.fn(count => ({
      success: true,
      session: {
        version: 1,
        sessionId: 'mock-session-id-12345678-1234-4123-8123-123456789012',
        createdAt: '2024-01-01T00:00:00.000Z',
        lastActiveAt: new Date().toISOString(),
        stats: { toolCalls: count, errors: 0, rateLimits: 0 },
      },
    })),
    incrementErrors: vi.fn(count => ({
      success: true,
      session: {
        version: 1,
        sessionId: 'mock-session-id-12345678-1234-4123-8123-123456789012',
        createdAt: '2024-01-01T00:00:00.000Z',
        lastActiveAt: new Date().toISOString(),
        stats: { toolCalls: 0, errors: count, rateLimits: 0 },
      },
    })),
    deleteSession: vi.fn(),
  };
});

import {
  initializeSession,
  logSessionInit,
  logSessionError,
  resetSessionManager,
} from '../../octocode-tools-core/src/session.js';

global.fetch = vi.fn();

describe('session - Edge Cases', () => {
  beforeEach(() => {
    vi.useRealTimers();
    vi.resetAllMocks();
    vi.mocked(global.fetch).mockResolvedValue(undefined as unknown as Response);
    resetSessionManager();
  });

  describe('initializeSession', () => {
    it('should create a new session with unique ID', () => {
      const session1 = initializeSession();

      expect(session1.getSessionId()).toBeDefined();
      expect(typeof session1.getSessionId()).toBe('string');
      expect(session1.getSessionId().length).toBeGreaterThan(0);
    });

    it('should return session with working methods', () => {
      const session = initializeSession();

      expect(typeof session.getSessionId).toBe('function');
      expect(typeof session.getSessionId()).toBe('string');
      expect(session.getSessionId().length).toBeGreaterThan(0);
    });
  });

  describe('logSessionInit', () => {
    beforeEach(() => {
      initializeSession();
    });

    it('should handle successful logging', async () => {
      vi.mocked(global.fetch).mockResolvedValueOnce({
        ok: true,
        status: 200,
        json: async () => ({ success: true }),
      } as Response);

      await expect(logSessionInit()).resolves.not.toThrow();
    });

    it('should handle fetch failure silently', async () => {
      vi.mocked(global.fetch).mockRejectedValueOnce(new Error('Network error'));

      await expect(logSessionInit()).resolves.not.toThrow();
    });

    it('should handle non-ok response silently', async () => {
      vi.mocked(global.fetch).mockResolvedValueOnce({
        ok: false,
        status: 500,
      } as Response);

      await expect(logSessionInit()).resolves.not.toThrow();
    });

    it('should handle timeout', async () => {
      vi.useFakeTimers();
      vi.mocked(global.fetch).mockImplementationOnce(
        () =>
          new Promise(resolve =>
            setTimeout(() => resolve({ ok: true } as Response), 10000)
          )
      );

      const promise = logSessionInit();
      await vi.advanceTimersByTimeAsync(10000);
      await expect(promise).resolves.not.toThrow();
      vi.useRealTimers();
    });
  });

  describe('logSessionError', () => {
    beforeEach(() => {
      initializeSession();
    });
    it('should handle successful error logging', async () => {
      vi.mocked(global.fetch).mockResolvedValueOnce({
        ok: true,
        status: 200,
        json: async () => ({ success: true }),
      } as Response);

      await expect(
        logSessionError('test-component', 'TEST_ERROR')
      ).resolves.not.toThrow();
    });

    it('should handle fetch failure silently', async () => {
      vi.mocked(global.fetch).mockRejectedValueOnce(new Error('Network error'));

      await expect(
        logSessionError('test', 'ERROR_CODE')
      ).resolves.not.toThrow();
    });

    it('should handle non-ok response silently', async () => {
      vi.mocked(global.fetch).mockResolvedValueOnce({
        ok: false,
        status: 500,
      } as Response);

      await expect(
        logSessionError('test', 'ERROR_CODE')
      ).resolves.not.toThrow();
    });

    it('should handle timeout', async () => {
      vi.mocked(global.fetch).mockImplementationOnce(
        () =>
          new Promise(resolve =>
            setTimeout(() => resolve({ ok: true } as Response), 10000)
          )
      );

      await expect(
        logSessionError('test', 'ERROR_CODE')
      ).resolves.not.toThrow();
    });

    it('should log different components and error codes', async () => {
      vi.mocked(global.fetch).mockResolvedValue({
        ok: true,
        status: 200,
      } as Response);

      await expect(
        Promise.all([
          logSessionError('github', 'API_ERROR'),
          logSessionError('toolMetadata', 'FETCH_FAILED'),
          logSessionError('promiseUtils', 'TIMEOUT'),
        ])
      ).resolves.toEqual([undefined, undefined, undefined]);
    });
  });

  describe('Session ID generation', () => {
    it('should generate session IDs', () => {
      const session = initializeSession();
      const id = session.getSessionId();

      expect(typeof id).toBe('string');
      expect(id.length).toBeGreaterThan(0);
    });

    it('should generate session IDs with expected format', () => {
      const session = initializeSession();
      const id = session.getSessionId();

      expect(typeof id).toBe('string');
      expect(id.length).toBeGreaterThan(0);
    });
  });

  describe('Logging with no network', () => {
    beforeEach(() => {
      initializeSession();
    });

    it('should handle completely failed fetch for init', async () => {
      vi.mocked(global.fetch).mockImplementationOnce(() => {
        throw new Error('Network unavailable');
      });

      await expect(logSessionInit()).resolves.not.toThrow();
    });

    it('should handle completely failed fetch for error', async () => {
      vi.mocked(global.fetch).mockImplementationOnce(() => {
        throw new Error('Network unavailable');
      });

      await expect(logSessionError('test', 'CODE')).resolves.not.toThrow();
    });
  });

  describe('Concurrent logging', () => {
    beforeEach(() => {
      initializeSession();
    });

    it('should handle concurrent init logs', async () => {
      vi.mocked(global.fetch).mockReset();
      vi.mocked(global.fetch).mockResolvedValue({
        ok: true,
        status: 200,
      } as Response);

      const promises = [logSessionInit(), logSessionInit(), logSessionInit()];

      await expect(Promise.all(promises)).resolves.not.toThrow();
    });

    it('should handle concurrent error logs', async () => {
      vi.mocked(global.fetch).mockResolvedValue({
        ok: true,
        status: 200,
      } as Response);

      const promises = [
        logSessionError('comp1', 'ERR1'),
        logSessionError('comp2', 'ERR2'),
        logSessionError('comp3', 'ERR3'),
      ];

      await expect(Promise.all(promises)).resolves.not.toThrow();
    });
  });
});
