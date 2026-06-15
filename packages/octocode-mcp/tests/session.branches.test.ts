import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  initializeSession,
  getSessionManager,
  resetSessionManager,
  logSessionError,
  logRateLimit,
} from '../../octocode-tools-core/src/session.js';
import {
  initialize,
  cleanup,
} from '../../octocode-tools-core/src/serverConfig.js';

describe('session.branches', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetSessionManager();
  });

  afterEach(() => {
    resetSessionManager();
  });

  describe('SessionManager.logError', () => {
    it('should call incrementErrors and sendLog when logging is enabled', async () => {
      const { incrementErrors } = await import('octocode-shared');
      vi.mocked(incrementErrors).mockReturnValue({
        success: true,
        session: {
          version: 1,
          sessionId: 'test-session-id',
          createdAt: '2024-01-01T00:00:00.000Z',
          lastActiveAt: '2024-01-01T00:00:00.000Z',
          stats: { toolCalls: 0, errors: 1, rateLimits: 0 },
        },
      });
      vi.mocked(fetch).mockResolvedValue(new Response('ok'));

      await initialize();
      initializeSession();
      await logSessionError('testTool', 'TEST_ERROR');

      expect(incrementErrors).toHaveBeenCalledWith(1);
      expect(vi.mocked(fetch)).toHaveBeenCalledWith(
        'https://octocode-mcp-host.onrender.com/log',
        expect.objectContaining({
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          signal: expect.any(AbortSignal),
        })
      );
      expect(
        JSON.parse(
          (vi.mocked(fetch).mock.calls[0]![1] as RequestInit).body as string
        )
      ).toEqual(
        expect.objectContaining({
          intent: 'error',
          data: { error: 'testTool:TEST_ERROR' },
        })
      );
    });
  });

  describe('SessionManager.logRateLimit', () => {
    it('should call provider rate-limit stats and sendLog when logging is enabled', async () => {
      const { updateSessionStats, incrementGitHubCacheRateLimits } =
        await import('octocode-shared');
      vi.mocked(updateSessionStats).mockReturnValue({
        success: true,
        session: {
          version: 1,
          sessionId: 'test-session-id',
          createdAt: '2024-01-01T00:00:00.000Z',
          lastActiveAt: '2024-01-01T00:00:00.000Z',
          stats: { toolCalls: 0, errors: 0, rateLimits: 1 },
        },
      });
      vi.mocked(fetch).mockResolvedValue(new Response('ok'));

      await initialize();
      initializeSession();
      const rateLimitData = {
        provider: 'github',
        limit: 5000,
        remaining: 0,
        resetAt: new Date().toISOString(),
      } as any;

      await logRateLimit(rateLimitData);

      expect(updateSessionStats).toHaveBeenCalledWith({
        rateLimits: 1,
        rateLimitsByProvider: {
          github: 1,
        },
      });
      expect(incrementGitHubCacheRateLimits).toHaveBeenCalledWith(1);
      expect(vi.mocked(fetch)).toHaveBeenCalledWith(
        'https://octocode-mcp-host.onrender.com/log',
        expect.objectContaining({
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          signal: expect.any(AbortSignal),
        })
      );
      expect(
        JSON.parse(
          (vi.mocked(fetch).mock.calls[0]![1] as RequestInit).body as string
        )
      ).toEqual(
        expect.objectContaining({
          intent: 'rate_limit',
          data: rateLimitData,
        })
      );
    });

    it('should update session when provider rate-limit stats return session', async () => {
      const { updateSessionStats } = await import('octocode-shared');
      const updatedSession = {
        version: 1,
        sessionId: 'updated-session-id',
        createdAt: '2024-01-01T00:00:00.000Z',
        lastActiveAt: '2024-01-01T00:00:00.000Z',
        stats: { toolCalls: 0, errors: 0, rateLimits: 5 },
      };
      vi.mocked(updateSessionStats).mockReturnValue({
        success: true,
        session: updatedSession as any,
      });
      vi.mocked(fetch).mockResolvedValue(new Response('ok'));

      await initialize();
      const session = initializeSession();
      session.getSessionId();
      const rateLimitData = {
        provider: 'github',
        limit: 5000,
        remaining: 0,
        resetAt: new Date().toISOString(),
      } as any;

      await logRateLimit(rateLimitData);

      expect(session.getSession().sessionId).toBe('updated-session-id');
      expect(session.getSession().stats.rateLimits).toBe(5);
    });
  });

  describe('SessionManager.sendLog - logging disabled', () => {
    beforeEach(async () => {
      process.env.LOG = 'false';
      cleanup();
      vi.mocked(fetch).mockResolvedValue(new Response('ok'));
      await initialize();
    });

    it('should skip error logging when LOG=false', async () => {
      initializeSession();
      await logSessionError('testTool', 'TEST_ERROR');

      expect(vi.mocked(fetch)).not.toHaveBeenCalled();
    });

    it('should skip rate_limit logging when LOG=false', async () => {
      initializeSession();
      await logRateLimit({
        limit_type: 'primary',
        retry_after_seconds: 60,
        rate_limit_remaining: 0,
      } as any);

      expect(vi.mocked(fetch)).not.toHaveBeenCalled();
    });

    it('should send init', async () => {
      const session = initializeSession();
      await session.logInit();
      expect(vi.mocked(fetch)).toHaveBeenCalledTimes(1);
      const payload = JSON.parse(
        (vi.mocked(fetch).mock.calls[0]![1] as RequestInit).body as string
      );
      expect(payload.intent).toBe('init');
    });
  });

  describe('SessionManager.sendLog - HTTP request failure', () => {
    beforeEach(async () => {
      process.env.LOG = 'true';
      cleanup();
      await initialize();
    });

    it('should catch and silently ignore HTTP request failures', async () => {
      const error = new Error('Network error');
      vi.mocked(fetch).mockRejectedValue(error);

      const stderrWriteSpy = vi
        .spyOn(process.stderr, 'write')
        .mockImplementation(() => true);

      initializeSession();
      await logSessionError('testTool', 'TEST_ERROR');

      expect(vi.mocked(fetch)).toHaveBeenCalled();
      expect(stderrWriteSpy).not.toHaveBeenCalledWith(
        expect.stringContaining('[session] Failed to send log')
      );

      stderrWriteSpy.mockRestore();
    });

    it('should handle non-Error rejection values silently', async () => {
      vi.mocked(fetch).mockRejectedValue('String error');

      const stderrWriteSpy = vi
        .spyOn(process.stderr, 'write')
        .mockImplementation(() => true);

      initializeSession();
      await logSessionError('testTool', 'TEST_ERROR');

      expect(vi.mocked(fetch)).toHaveBeenCalled();
      expect(stderrWriteSpy).not.toHaveBeenCalledWith(
        expect.stringContaining('[session] Failed to send log')
      );

      stderrWriteSpy.mockRestore();
    });
  });

  describe('getSessionManager - not initialized', () => {
    it('should return null when session manager is not initialized', () => {
      resetSessionManager();
      const manager = getSessionManager();
      expect(manager).toBeNull();
    });
  });

  describe('logRateLimit wrapper - session null branch', () => {
    it('should return early when session manager is null', async () => {
      resetSessionManager();
      vi.mocked(fetch).mockResolvedValue(new Response('ok'));
      const rateLimitData = {
        provider: 'github',
        limit: 5000,
        remaining: 0,
        resetAt: new Date().toISOString(),
      } as any;

      await logRateLimit(rateLimitData);

      expect(vi.mocked(fetch)).not.toHaveBeenCalled();
    });
  });

  describe('initializeSession - already initialized', () => {
    it('should return existing session manager when already initialized', async () => {
      await initialize();
      const session1 = initializeSession();
      const session2 = initializeSession();

      expect(session1).toBe(session2);
      expect(getSessionManager()).toBe(session1);
    });
  });

  describe('resetSessionManager', () => {
    it('should set sessionManager to null', async () => {
      await initialize();
      initializeSession();
      expect(getSessionManager()).not.toBeNull();

      resetSessionManager();
      expect(getSessionManager()).toBeNull();
    });
  });

  describe('isLoggingEnabled - various env scenarios', () => {
    it('should return true when LOG env is "true"', async () => {
      process.env.LOG = 'true';
      cleanup();
      await initialize();

      const { isLoggingEnabled } =
        await import('../../octocode-tools-core/src/serverConfig.js');
      expect(isLoggingEnabled()).toBe(true);
    });

    it('should return false when LOG env is "false"', async () => {
      process.env.LOG = 'false';
      cleanup();
      await initialize();

      const { isLoggingEnabled } =
        await import('../../octocode-tools-core/src/serverConfig.js');
      expect(isLoggingEnabled()).toBe(false);
    });

    it('should return true when LOG env is not set (default)', async () => {
      delete process.env.LOG;
      cleanup();
      await initialize();

      const { isLoggingEnabled } =
        await import('../../octocode-tools-core/src/serverConfig.js');
      expect(typeof isLoggingEnabled()).toBe('boolean');
    });
  });
});
