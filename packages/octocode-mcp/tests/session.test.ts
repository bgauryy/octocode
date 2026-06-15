vi.mock('octocode-shared', async importOriginal => {
  const actual = await importOriginal<typeof import('octocode-shared')>();
  return {
    ...actual,
    getOrCreateSession: vi.fn(() => ({
      version: 1,
      sessionId: '12345678-1234-4123-8123-123456789012',
      createdAt: '2024-01-01T00:00:00.000Z',
      lastActiveAt: '2024-01-01T00:00:00.000Z',
      stats: { toolCalls: 0, errors: 0, rateLimits: 0 },
    })),
    incrementToolCalls: vi.fn(count => ({
      success: true,
      session: {
        version: 1,
        sessionId: '12345678-1234-4123-8123-123456789012',
        createdAt: '2024-01-01T00:00:00.000Z',
        lastActiveAt: new Date().toISOString(),
        stats: { toolCalls: count, errors: 0, rateLimits: 0 },
      },
    })),
    incrementErrors: vi.fn(count => ({
      success: true,
      session: {
        version: 1,
        sessionId: '12345678-1234-4123-8123-123456789012',
        createdAt: '2024-01-01T00:00:00.000Z',
        lastActiveAt: new Date().toISOString(),
        stats: { toolCalls: 0, errors: count, rateLimits: 0 },
      },
    })),
    incrementRateLimits: vi.fn(count => ({
      success: true,
      session: {
        version: 1,
        sessionId: '12345678-1234-4123-8123-123456789012',
        createdAt: '2024-01-01T00:00:00.000Z',
        lastActiveAt: new Date().toISOString(),
        stats: { toolCalls: 0, errors: 0, rateLimits: count },
      },
    })),
    updateSessionStats: vi.fn(updates => ({
      success: true,
      session: {
        version: 1,
        sessionId: '12345678-1234-4123-8123-123456789012',
        createdAt: '2024-01-01T00:00:00.000Z',
        lastActiveAt: new Date().toISOString(),
        stats: {
          toolCalls: 0,
          errors: 0,
          rateLimits: updates.rateLimits ?? 0,
          ...updates,
        },
      },
    })),
    incrementRateLimitByProvider: vi.fn((provider, count) => ({
      success: true,
      session: {
        version: 1,
        sessionId: '12345678-1234-4123-8123-123456789012',
        createdAt: '2024-01-01T00:00:00.000Z',
        lastActiveAt: new Date().toISOString(),
        stats: {
          toolCalls: 0,
          errors: 0,
          rateLimits: count,
          rateLimitsByProvider: { [provider]: count },
        },
      },
    })),
    incrementToolCharSavings: vi.fn(() => ({ success: true })),
    incrementGitHubCacheHits: vi.fn(() => ({ success: true })),
    incrementGitHubCacheRateLimits: vi.fn(() => ({ success: true })),
    incrementPackageRegistryFailures: vi.fn(() => ({ success: true })),
  };
});

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  initializeSession,
  getSessionManager,
  logSessionInit,
  logToolCall,
  logSessionError,
  resetSessionManager,
} from '../../octocode-tools-core/src/session.js';
import {
  initialize,
  cleanup,
} from '../../octocode-tools-core/src/serverConfig.js';
import { TOOL_NAMES } from '../../octocode-tools-core/src/tools/toolMetadata/proxies.js';

process.env.LOG = 'true';

describe('Session Management', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetSessionManager();
    cleanup();
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  describe('Session Initialization', () => {
    it('should create a session with UUID', () => {
      const session = initializeSession();
      const sessionId = session.getSessionId();
      const isValidUUID =
        /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(
          sessionId
        );
      expect(typeof session).toEqual('object');
      expect(typeof sessionId).toEqual('string');
      expect(isValidUUID).toEqual(true);
    });

    it('should return the same session instance on multiple calls', () => {
      const session1 = initializeSession();
      const session2 = initializeSession();
      expect(session1).toBe(session2);
      expect(session1.getSessionId()).toBe(session2.getSessionId());
    });

    it('should be accessible via getSessionManager', () => {
      const session = initializeSession();
      const retrieved = getSessionManager();
      expect(retrieved).toBe(session);
    });
  });

  describe('Session Logging', () => {
    beforeEach(async () => {
      process.env.LOG = 'true';
      process.env.GITHUB_TOKEN = 'mock-token';
      await initialize();
      initializeSession();

      vi.mocked(fetch).mockResolvedValue(new Response('ok'));
    });

    it('should log session initialization', async () => {
      const session = initializeSession();
      await logSessionInit();

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
          sessionId: session.getSessionId(),
          intent: 'init',
          data: {},
          timestamp: expect.stringMatching(
            /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/
          ),
          version: expect.any(String),
        })
      );
    });

    it('should log tool calls', async () => {
      const session = initializeSession();
      await logToolCall(TOOL_NAMES.GITHUB_SEARCH_CODE, []);

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
          sessionId: session.getSessionId(),
          intent: 'tool_call',
          data: expect.objectContaining({
            tool_name: TOOL_NAMES.GITHUB_SEARCH_CODE,
            repos: [],
            provider: 'github',
          }),
          timestamp: expect.stringMatching(
            /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/
          ),
          version: expect.any(String),
        })
      );
    });

    it('should log tool calls with repos', async () => {
      const session = initializeSession();
      await logToolCall(TOOL_NAMES.GITHUB_SEARCH_CODE, ['my-owner/my-repo']);

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
          sessionId: session.getSessionId(),
          intent: 'tool_call',
          data: expect.objectContaining({
            tool_name: TOOL_NAMES.GITHUB_SEARCH_CODE,
            repos: ['[redacted]'],
            provider: 'github',
          }),
          timestamp: expect.stringMatching(
            /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/
          ),
          version: expect.any(String),
        })
      );
    });

    it('should log tool calls with research fields', async () => {
      const session = initializeSession();
      await logToolCall(
        TOOL_NAMES.GITHUB_SEARCH_CODE,
        ['my-owner/my-repo'],
        'Find authentication patterns',
        'Locate login implementation',
        'Need to understand auth flow'
      );

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
          sessionId: session.getSessionId(),
          intent: 'tool_call',
          data: expect.objectContaining({
            tool_name: TOOL_NAMES.GITHUB_SEARCH_CODE,
            repos: ['[redacted]'],
            provider: 'github',
          }),
          timestamp: expect.stringMatching(
            /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/
          ),
          version: expect.any(String),
        })
      );
    });

    it('should log tool calls with partial research fields', async () => {
      const session = initializeSession();
      await logToolCall(
        TOOL_NAMES.GITHUB_SEARCH_CODE,
        ['my-owner/my-repo'],
        'Find authentication patterns',
        undefined,
        'Need to understand auth flow'
      );

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
          sessionId: session.getSessionId(),
          intent: 'tool_call',
          data: expect.objectContaining({
            tool_name: TOOL_NAMES.GITHUB_SEARCH_CODE,
            repos: ['[redacted]'],
            provider: 'github',
          }),
          timestamp: expect.stringMatching(
            /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/
          ),
          version: expect.any(String),
        })
      );
    });

    it('should log tool calls without research fields when all are undefined', async () => {
      const session = initializeSession();
      await logToolCall(
        TOOL_NAMES.GITHUB_SEARCH_CODE,
        ['my-owner/my-repo'],
        undefined,
        undefined,
        undefined
      );

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
          sessionId: session.getSessionId(),
          intent: 'tool_call',
          data: expect.objectContaining({
            tool_name: TOOL_NAMES.GITHUB_SEARCH_CODE,
            repos: ['[redacted]'],
            provider: 'github',
          }),
          timestamp: expect.stringMatching(
            /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/
          ),
          version: expect.any(String),
        })
      );
    });

    it('should log errors', async () => {
      const session = initializeSession();
      await logSessionError('test', 'TEST_ERROR');

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
          sessionId: session.getSessionId(),
          intent: 'error',
          data: { error: 'test:TEST_ERROR' },
          timestamp: expect.stringMatching(
            /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/
          ),
          version: expect.any(String),
        })
      );
    });

    it('should handle logging failures gracefully', async () => {
      vi.mocked(fetch).mockRejectedValue(new Error('Network error'));

      initializeSession();

      const result1 = await logSessionInit();
      const result2 = await logToolCall('test_tool', []);
      const result3 = await logSessionError('test', 'TEST_ERROR');

      expect(result1).toEqual(undefined);
      expect(result2).toEqual(undefined);
      expect(result3).toEqual(undefined);
    });

    it('should always log init', async () => {
      cleanup();
      process.env.LOG = 'false';
      process.env.GITHUB_TOKEN = 'mock-token';
      await initialize();
      initializeSession();
      vi.mocked(fetch).mockResolvedValue(new Response('ok'));

      await logSessionInit();
      expect(vi.mocked(fetch)).toHaveBeenCalledTimes(1);
      const payload = JSON.parse(
        (vi.mocked(fetch).mock.calls[0]![1] as RequestInit).body as string
      );
      expect(payload.intent).toBe('init');

      vi.mocked(fetch).mockClear();
      await logToolCall(TOOL_NAMES.GITHUB_SEARCH_CODE, []);
      expect(vi.mocked(fetch)).not.toHaveBeenCalled();

      process.env.LOG = 'true';
    });

    it('should not log if session is not initialized', async () => {
      resetSessionManager();
      await logSessionInit();
      await logToolCall('test_tool', []);
      await logSessionError('test', 'TEST_ERROR');

      expect(vi.mocked(fetch)).not.toHaveBeenCalled();
    });
  });

  describe('Session Data Structure', () => {
    beforeEach(async () => {
      vi.mocked(fetch).mockResolvedValue(new Response('ok'));
      process.env.LOG = 'true';
      process.env.GITHUB_TOKEN = 'mock-token';
      await initialize();
      initializeSession();
    });
    it('should create proper session data structure for init', async () => {
      const session = initializeSession();
      await session.logInit();

      const call = vi.mocked(fetch).mock.calls[0];
      const payload = call?.[1]
        ? JSON.parse((call[1] as RequestInit).body as string)
        : undefined;

      expect(payload).toEqual({
        sessionId: expect.stringMatching(/^[0-9a-f-]{36}$/i),
        intent: 'init',
        data: {},
        timestamp: expect.stringMatching(
          /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/
        ),
        version: expect.any(String),
      });
    });

    it('should create proper session data structure for tool calls', async () => {
      const session = initializeSession();
      await session.logToolCall(TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES, []);

      const call = vi.mocked(fetch).mock.calls[0];
      const payload = call?.[1]
        ? JSON.parse((call[1] as RequestInit).body as string)
        : undefined;

      expect(payload).toEqual({
        sessionId: expect.stringMatching(/^[0-9a-f-]{36}$/i),
        intent: 'tool_call',
        data: expect.objectContaining({
          tool_name: TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
          repos: [],
          provider: 'github',
        }),
        timestamp: expect.stringMatching(
          /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/
        ),
        version: expect.any(String),
      });
    });

    it('should create proper session data structure for tool calls with repos', async () => {
      const session = initializeSession();
      await session.logToolCall(TOOL_NAMES.GITHUB_FETCH_CONTENT, [
        'test-owner/test-repo',
      ]);

      const call = vi.mocked(fetch).mock.calls[0];
      const payload = call?.[1]
        ? JSON.parse((call[1] as RequestInit).body as string)
        : undefined;

      expect(payload).toEqual({
        sessionId: expect.stringMatching(/^[0-9a-f-]{36}$/i),
        intent: 'tool_call',
        data: expect.objectContaining({
          tool_name: TOOL_NAMES.GITHUB_FETCH_CONTENT,
          repos: ['[redacted]'],
          provider: 'github',
        }),
        timestamp: expect.stringMatching(
          /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/
        ),
        version: expect.any(String),
      });
    });

    it('should create proper session data structure with all research fields', async () => {
      const session = initializeSession();
      await session.logToolCall(
        TOOL_NAMES.GITHUB_SEARCH_CODE,
        ['owner/repo'],
        'Main goal',
        'Specific goal',
        'Reasoning text'
      );

      const call = vi.mocked(fetch).mock.calls[0];
      const payload = call?.[1]
        ? JSON.parse((call[1] as RequestInit).body as string)
        : undefined;

      expect(payload).toEqual({
        sessionId: expect.stringMatching(/^[0-9a-f-]{36}$/i),
        intent: 'tool_call',
        data: expect.objectContaining({
          tool_name: TOOL_NAMES.GITHUB_SEARCH_CODE,
          repos: ['[redacted]'],
          provider: 'github',
        }),
        timestamp: expect.stringMatching(
          /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/
        ),
        version: expect.any(String),
      });
    });

    it('should create proper session data structure with only mainResearchGoal', async () => {
      const session = initializeSession();
      await session.logToolCall(
        TOOL_NAMES.GITHUB_SEARCH_CODE,
        ['owner/repo'],
        'Main goal only'
      );

      const call = vi.mocked(fetch).mock.calls[0];
      const payload = call?.[1]
        ? JSON.parse((call[1] as RequestInit).body as string)
        : undefined;

      expect(payload).toEqual({
        sessionId: expect.stringMatching(/^[0-9a-f-]{36}$/i),
        intent: 'tool_call',
        data: expect.objectContaining({
          tool_name: TOOL_NAMES.GITHUB_SEARCH_CODE,
          repos: ['[redacted]'],
          provider: 'github',
        }),
        timestamp: expect.stringMatching(
          /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/
        ),
        version: expect.any(String),
      });
    });

    it('should create proper session data structure for errors', async () => {
      const session = initializeSession();
      await session.logError('test', 'CONNECTION_FAILED');

      const call = vi.mocked(fetch).mock.calls[0];
      const payload = call?.[1]
        ? JSON.parse((call[1] as RequestInit).body as string)
        : undefined;

      expect(payload).toEqual({
        sessionId: expect.stringMatching(/^[0-9a-f-]{36}$/i),
        intent: 'error',
        data: { error: 'test:CONNECTION_FAILED' },
        timestamp: expect.stringMatching(
          /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/
        ),
        version: expect.any(String),
      });
    });
  });
});
