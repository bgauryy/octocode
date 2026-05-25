import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { deleteSession, _resetSessionState } from 'octocode-shared';

// LOG environment variable is set in individual tests

import {
  initializeSession,
  logSessionInit,
  logToolCall,
  logSessionError,
  logRateLimit,
  resetSessionManager,
} from '../src/session.js';
import { TOOL_NAMES } from '../src/tools/toolMetadata/proxies.js';
import { initialize, cleanup } from '../src/serverConfig.js';
import type { RateLimitData } from '../src/types.js';

describe('Session Logging Control', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // Reset mock session state for each test (done by setup.ts beforeEach)
    resetSessionManager();
  });

  afterEach(() => {
    resetSessionManager();
  });

  describe('When logging is enabled', () => {
    beforeEach(async () => {
      vi.mocked(fetch).mockResolvedValue(new Response('ok'));
      await initialize();
    });

    it('should send session init log', async () => {
      const session = initializeSession();
      await logSessionInit();

      const callArgs = vi.mocked(fetch).mock.calls[0]!;
      const payloadData = JSON.parse(
        (callArgs[1] as RequestInit).body as string
      ) as {
        sessionId: string;
        intent: string;
        data: object;
        timestamp: string;
      };
      expect(callArgs[0]).toEqual('https://octocode-mcp-host.onrender.com/log');
      expect(payloadData.sessionId).toEqual(session.getSessionId());
      expect(payloadData.intent).toEqual('init');
      expect(payloadData.data).toEqual({});
      expect(typeof payloadData.timestamp).toEqual('string');
    });

    it('should send tool call log', async () => {
      const session = initializeSession();
      await logToolCall(TOOL_NAMES.GITHUB_SEARCH_CODE, []);

      const callArgs = vi.mocked(fetch).mock.calls[0]!;
      const payloadData = JSON.parse(
        (callArgs[1] as RequestInit).body as string
      ) as {
        sessionId: string;
        intent: string;
        data: { tool_name: string; repos: string[] };
        timestamp: string;
      };
      expect(callArgs[0]).toEqual('https://octocode-mcp-host.onrender.com/log');
      expect(payloadData.sessionId).toEqual(session.getSessionId());
      expect(payloadData.intent).toEqual('tool_call');
      expect(payloadData.data).toEqual(
        expect.objectContaining({
          tool_name: TOOL_NAMES.GITHUB_SEARCH_CODE,
          repos: [],
          provider: 'github',
        })
      );
      expect(typeof payloadData.timestamp).toEqual('string');
    });

    it('should send tool call log with repos', async () => {
      const session = initializeSession();
      await logToolCall(TOOL_NAMES.GITHUB_FETCH_CONTENT, ['my-owner/my-repo']);

      const callArgs = vi.mocked(fetch).mock.calls[0]!;
      const payloadData = JSON.parse(
        (callArgs[1] as RequestInit).body as string
      ) as {
        sessionId: string;
        intent: string;
        data: { tool_name: string; repos: string[] };
        timestamp: string;
      };
      expect(callArgs[0]).toEqual('https://octocode-mcp-host.onrender.com/log');
      expect(payloadData.sessionId).toEqual(session.getSessionId());
      expect(payloadData.intent).toEqual('tool_call');
      expect(payloadData.data).toEqual(
        expect.objectContaining({
          tool_name: TOOL_NAMES.GITHUB_FETCH_CONTENT,
          repos: ['[redacted]'],
          provider: 'github',
        })
      );
      expect(typeof payloadData.timestamp).toEqual('string');
    });

    it('should send error log', async () => {
      const session = initializeSession();
      await logSessionError('test', 'TEST_ERROR');

      const callArgs = vi.mocked(fetch).mock.calls[0]!;
      const payloadData = JSON.parse(
        (callArgs[1] as RequestInit).body as string
      ) as {
        sessionId: string;
        intent: string;
        data: { error: string };
        timestamp: string;
      };
      expect(callArgs[0]).toEqual('https://octocode-mcp-host.onrender.com/log');
      expect(payloadData.sessionId).toEqual(session.getSessionId());
      expect(payloadData.intent).toEqual('error');
      expect(payloadData.data).toEqual({ error: 'test:TEST_ERROR' });
      expect(typeof payloadData.timestamp).toEqual('string');
    });
  });

  describe('When logging is disabled (LOG=false)', () => {
    beforeEach(async () => {
      process.env.LOG = 'false';
      cleanup();
      vi.mocked(fetch).mockResolvedValue(new Response('ok'));
      await initialize();
    });

    it('should always send session init log even when LOG=false', async () => {
      initializeSession();
      await logSessionInit();
      expect(vi.mocked(fetch)).toHaveBeenCalledTimes(1);
      const payload = JSON.parse(
        (vi.mocked(fetch).mock.calls[0]![1] as RequestInit).body as string
      );
      expect(payload.intent).toBe('init');
    });

    it('should NOT send tool call log', async () => {
      initializeSession();
      await logToolCall(TOOL_NAMES.GITHUB_SEARCH_CODE, []);

      expect(vi.mocked(fetch)).not.toHaveBeenCalled();
    });

    it('should NOT send tool call log with repos', async () => {
      initializeSession();
      await logToolCall(TOOL_NAMES.GITHUB_FETCH_CONTENT, ['my-owner/my-repo']);

      expect(vi.mocked(fetch)).not.toHaveBeenCalled();
    });

    it('should NOT send error log', async () => {
      initializeSession();
      await logSessionError('test', 'TEST_ERROR');

      expect(vi.mocked(fetch)).not.toHaveBeenCalled();
    });

    it('should send init but skip tool calls and errors when LOG=false', async () => {
      initializeSession();

      await logSessionInit();
      expect(vi.mocked(fetch)).toHaveBeenCalledTimes(1);
      const payload = JSON.parse(
        (vi.mocked(fetch).mock.calls[0]![1] as RequestInit).body as string
      );
      expect(payload.intent).toBe('init');

      vi.mocked(fetch).mockClear();
      await logToolCall('test_tool', []);
      await logToolCall('test_tool', ['owner/repo']);
      await logSessionError('test', 'TEST_ERROR');
      expect(vi.mocked(fetch)).not.toHaveBeenCalled();
    });
  });

  describe('Dynamic logging control', () => {
    beforeEach(async () => {
      await initialize();
    });

    it('should respect logging state changes', async () => {
      // Start with logging enabled
      process.env.LOG = 'true';
      cleanup();
      await initialize();
      resetSessionManager();
      initializeSession();

      await logToolCall('tool1', []);
      expect(vi.mocked(fetch)).toHaveBeenCalledTimes(1);

      // Note: In the current implementation, logging state is cached
      // and cannot be changed dynamically without reinitializing
      // This test documents the current behavior
      await logToolCall('tool2', []);
      expect(vi.mocked(fetch)).toHaveBeenCalledTimes(2); // Still called since LOG=true
    });
  });

  describe('Error handling with logging disabled', () => {
    beforeEach(async () => {
      process.env.LOG = 'false';
      cleanup();
      await initialize();
    });

    it('should not throw errors even if fetch would fail', async () => {
      vi.mocked(fetch).mockRejectedValue(new Error('Network error'));

      initializeSession();

      await logSessionInit();
      // init always fires (even with LOG=false), but failure is swallowed
      expect(vi.mocked(fetch)).toHaveBeenCalledTimes(1);

      vi.mocked(fetch).mockClear();
      await logToolCall('test', []);
      await logSessionError('test', 'TEST_ERROR');
      expect(vi.mocked(fetch)).not.toHaveBeenCalled();
    });
  });

  describe('Session ID generation with logging disabled', () => {
    beforeEach(async () => {
      process.env.LOG = 'false';
      cleanup();
      await initialize();
    });

    it('should persist session ID across restarts when logging is disabled', async () => {
      // With persistence, the same session ID is reused
      resetSessionManager();
      const session1 = initializeSession();
      const id1 = session1.getSessionId();

      resetSessionManager();
      const session2 = initializeSession();
      const id2 = session2.getSessionId();

      expect(typeof id1).toEqual('string');
      expect(id1.length).toEqual(36);
      expect(typeof id2).toEqual('string');
      expect(id2.length).toEqual(36);
      // With persistence, session ID should be the SAME
      expect(id1).toBe(id2);
    });

    it('should generate new session ID when session is deleted', async () => {
      resetSessionManager();
      const session1 = initializeSession();
      const id1 = session1.getSessionId();

      // Delete persisted session to force new ID
      resetSessionManager();
      deleteSession();
      const session2 = initializeSession();
      const id2 = session2.getSessionId();

      expect(typeof id1).toEqual('string');
      expect(id1.length).toEqual(36);
      expect(typeof id2).toEqual('string');
      expect(id2.length).toEqual(36);
      // After deleting session, a new ID is generated
      expect(id1).not.toBe(id2);
    });
  });

  describe('Tool call logging is blocked end-to-end when LOG=false', () => {
    beforeEach(async () => {
      process.env.LOG = 'false';
      cleanup();
      vi.mocked(fetch).mockResolvedValue(new Response('ok'));
      await initialize();
    });

    it('should NOT send tool_call for github tools', async () => {
      initializeSession();
      await logToolCall(TOOL_NAMES.GITHUB_SEARCH_CODE, ['facebook/react']);

      expect(vi.mocked(fetch)).not.toHaveBeenCalled();
    });

    it('should NOT send tool_call for github tools with research fields', async () => {
      initializeSession();
      await logToolCall(
        TOOL_NAMES.GITHUB_FETCH_CONTENT,
        ['microsoft/vscode'],
        'Research vscode extensions',
        'Find API surface',
        'Need to understand extension host'
      );

      expect(vi.mocked(fetch)).not.toHaveBeenCalled();
    });

    it('should NOT send tool_call for multiple sequential calls', async () => {
      initializeSession();

      await logToolCall(TOOL_NAMES.GITHUB_SEARCH_CODE, ['owner1/repo1']);
      await logToolCall(TOOL_NAMES.GITHUB_FETCH_CONTENT, ['owner2/repo2']);
      await logToolCall(TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES, []);

      expect(vi.mocked(fetch)).not.toHaveBeenCalled();
    });

    it('should NOT send tool_call even with empty repos', async () => {
      initializeSession();
      await logToolCall(TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES, []);

      expect(vi.mocked(fetch)).not.toHaveBeenCalled();
    });

    it('should still update persistent stats even when LOG=false', async () => {
      const { incrementToolCalls } = await import('octocode-shared');
      initializeSession();

      await logToolCall(TOOL_NAMES.GITHUB_SEARCH_CODE, ['owner/repo']);

      // Stats are always updated locally, even when remote logging is off
      expect(incrementToolCalls).toHaveBeenCalledWith(1);
      // But no remote call was made
      expect(vi.mocked(fetch)).not.toHaveBeenCalled();
    });
  });

  describe('Session init follows LOG setting', () => {
    afterEach(() => {
      resetSessionManager();
    });

    describe('with LOG=false', () => {
      beforeEach(async () => {
        process.env.LOG = 'false';
        cleanup();
        vi.mocked(fetch).mockResolvedValue(new Response('ok'));
        await initialize();
      });

      it('should always send init even with LOG=false', async () => {
        initializeSession();
        await logSessionInit();
        expect(vi.mocked(fetch)).toHaveBeenCalledTimes(1);
        const payload = JSON.parse(
          (vi.mocked(fetch).mock.calls[0]![1] as RequestInit).body as string
        );
        expect(payload.intent).toBe('init');
      });

      it('should NOT send rate_limit when LOG=false', async () => {
        initializeSession();
        const rateLimitData: RateLimitData = {
          limit_type: 'primary',
          retry_after_seconds: 60,
          rate_limit_remaining: 0,
        };
        await logRateLimit(rateLimitData);

        expect(vi.mocked(fetch)).not.toHaveBeenCalled();
      });

      it('should send init but skip other intents when LOG=false', async () => {
        initializeSession();
        const rateLimitData: RateLimitData = {
          limit_type: 'primary',
          retry_after_seconds: 60,
        };

        await logSessionInit();
        expect(vi.mocked(fetch)).toHaveBeenCalledTimes(1);

        vi.mocked(fetch).mockClear();
        await logToolCall(TOOL_NAMES.GITHUB_SEARCH_CODE, ['owner/repo']);
        await logSessionError('github', 'API_ERROR');
        await logRateLimit(rateLimitData);

        expect(vi.mocked(fetch)).not.toHaveBeenCalled();
      });

      it('should send multiple init calls even when LOG=false', async () => {
        initializeSession();

        await logSessionInit();
        await logSessionInit();
        expect(vi.mocked(fetch)).toHaveBeenCalledTimes(2);
      });
    });

    describe('with LOG=0', () => {
      beforeEach(async () => {
        process.env.LOG = '0';
        cleanup();
        vi.mocked(fetch).mockResolvedValue(new Response('ok'));
        await initialize();
      });

      it('should always send init even when LOG=0', async () => {
        initializeSession();
        await logSessionInit();
        expect(vi.mocked(fetch)).toHaveBeenCalledTimes(1);
        const payload = JSON.parse(
          (vi.mocked(fetch).mock.calls[0]![1] as RequestInit).body as string
        );
        expect(payload.intent).toBe('init');
      });

      it('should NOT send tool_call when LOG=0', async () => {
        initializeSession();
        await logToolCall(TOOL_NAMES.GITHUB_SEARCH_CODE, []);

        expect(vi.mocked(fetch)).not.toHaveBeenCalled();
      });

      it('should NOT send error when LOG=0', async () => {
        initializeSession();
        await logSessionError('test', 'ERROR');

        expect(vi.mocked(fetch)).not.toHaveBeenCalled();
      });
    });

    describe('with LOG=true (all intents logged)', () => {
      beforeEach(async () => {
        process.env.LOG = 'true';
        cleanup();
        vi.mocked(fetch).mockResolvedValue(new Response('ok'));
        await initialize();
      });

      it('should send all intents when LOG=true', async () => {
        initializeSession();

        await logSessionInit();
        await logToolCall(TOOL_NAMES.GITHUB_SEARCH_CODE, ['owner/repo']);
        await logSessionError('test', 'ERROR');

        expect(vi.mocked(fetch)).toHaveBeenCalledTimes(3);

        const intents = vi
          .mocked(fetch)
          .mock.calls.map(
            call => JSON.parse((call[1] as RequestInit).body as string).intent
          );
        expect(intents).toEqual(['init', 'tool_call', 'error']);
      });
    });

    describe('graceful handling when LOG=false', () => {
      beforeEach(async () => {
        process.env.LOG = 'false';
        cleanup();
        await initialize();
      });

      it('should attempt network call for init even when LOG=false', async () => {
        vi.mocked(fetch).mockRejectedValue(new Error('Connection refused'));

        const stderrSpy = vi
          .spyOn(process.stderr, 'write')
          .mockImplementation(() => true);

        initializeSession();
        await logSessionInit();
        // init always fires
        expect(vi.mocked(fetch)).toHaveBeenCalledTimes(1);
        expect(stderrSpy).not.toHaveBeenCalled();

        stderrSpy.mockRestore();
      });

      it('should handle non-Error rejection gracefully for init when LOG=false', async () => {
        vi.mocked(fetch).mockRejectedValue('timeout');

        const stderrSpy = vi
          .spyOn(process.stderr, 'write')
          .mockImplementation(() => true);

        initializeSession();
        await logSessionInit();
        // init always fires
        expect(vi.mocked(fetch)).toHaveBeenCalledTimes(1);
        expect(stderrSpy).not.toHaveBeenCalled();

        stderrSpy.mockRestore();
      });

      it('should not throw when logging is disabled', async () => {
        vi.mocked(fetch).mockRejectedValue(new Error('Network error'));

        initializeSession();

        await expect(logSessionInit()).resolves.not.toThrow();
        await expect(logToolCall('tool', [])).resolves.not.toThrow();
        await expect(logSessionError('test', 'ERR')).resolves.not.toThrow();
        // init fires (1 call), tool_call and error are suppressed
        expect(vi.mocked(fetch)).toHaveBeenCalledTimes(1);
      });
    });
  });
});
