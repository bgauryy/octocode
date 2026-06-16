import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import {
  withBasicSecurityValidation,
  withSecurityValidation,
  configureSecurity,
} from '../src/withSecurityValidation.js';

function makeDeepObject(depth: number): Record<string, unknown> {
  return depth <= 0 ? {} : { x: makeDeepObject(depth - 1) };
}

function makeCircularObject(): Record<string, unknown> {
  const obj: Record<string, unknown> = { a: 1 };
  obj.self = obj;
  return obj;
}

const mockLogToolCall = vi.fn().mockResolvedValue(undefined);
const mockLogSessionError = vi.fn().mockResolvedValue(undefined);
const mockIsLoggingEnabled = vi.fn().mockReturnValue(false);
const GITHUB_SEARCH_CODE = 'ghSearchCode';
const GITHUB_SEARCH_REPOSITORIES = 'ghSearchRepos';

describe('withSecurityValidation - Additional Coverage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockIsLoggingEnabled.mockReturnValue(false);
    configureSecurity({
      logToolCall: mockLogToolCall,
      logSessionError: mockLogSessionError,
      isLoggingEnabled: mockIsLoggingEnabled,
    });
  });

  afterEach(() => {
    configureSecurity({
      logToolCall: undefined,
      logSessionError: undefined,
      isLoggingEnabled: undefined,
      sanitizer: undefined,
    });
  });

  describe('withBasicSecurityValidation', () => {
    it('should successfully validate and execute handler with clean input', async () => {
      const mockHandler = vi.fn().mockResolvedValue({
        content: [{ type: 'text' as const, text: 'success' }],
        isError: false,
      });

      const wrappedHandler = withBasicSecurityValidation(mockHandler);
      const result = await wrappedHandler(
        { query: 'test' },
        { signal: new AbortController().signal }
      );

      expect(mockHandler).toHaveBeenCalledWith({ query: 'test' });
      expect(result).toHaveProperty('content');
    });

    it('should reject dangerous parameter keys (real Rust validation)', async () => {
      const mockHandler = vi.fn();

      const wrappedHandler = withBasicSecurityValidation(mockHandler);
      const result = await wrappedHandler(
        { constructor: 'evil', prototype: 'bad' } as Record<string, unknown>,
        { signal: new AbortController().signal }
      );

      expect(mockHandler).not.toHaveBeenCalled();
      expect(result.isError).toBe(true);
      const text = (result.content[0] as { text: string }).text;
      expect(text).toContain('Security validation failed');
      expect(text).toContain('Dangerous parameter key blocked');
    });

    it('should reject circular references (real Rust validation)', async () => {
      const mockHandler = vi.fn();
      const wrappedHandler = withBasicSecurityValidation(mockHandler);
      const result = await wrappedHandler(makeCircularObject());

      expect(mockHandler).not.toHaveBeenCalled();
      expect(result.isError).toBe(true);
      const text = (result.content[0] as { text: string }).text;
      expect(text).toContain('Security validation failed');
      expect(text).toContain('Circular reference detected');
    });

    it('should reject objects exceeding max nesting depth (real Rust validation)', async () => {
      const mockHandler = vi.fn();
      const wrappedHandler = withBasicSecurityValidation(mockHandler);
      const result = await wrappedHandler(makeDeepObject(22));

      expect(mockHandler).not.toHaveBeenCalled();
      expect(result.isError).toBe(true);
    });

    it('should handle handler errors gracefully', async () => {
      const mockHandler = vi
        .fn()
        .mockRejectedValue(new Error('Handler execution failed'));
      const wrappedHandler = withBasicSecurityValidation(mockHandler);

      const result = await wrappedHandler(
        { query: 'test' },
        { signal: new AbortController().signal }
      );

      expect(result.isError).toBe(true);
      const text = (result.content[0] as { text: string }).text;
      expect(text).toContain('Handler execution failed');
    });
  });

  describe('withSecurityValidation - Logging Integration', () => {
    it('should log tool call when logging is enabled and repos are found', async () => {
      const mockHandler = vi.fn().mockResolvedValue({
        content: [{ type: 'text' as const, text: 'success' }],
        isError: false,
      });

      mockIsLoggingEnabled.mockReturnValue(true);
      const wrappedHandler = withSecurityValidation(
        GITHUB_SEARCH_CODE,
        mockHandler
      );

      await wrappedHandler(
        {
          queries: [
            { owner: 'facebook', repo: 'react' },
            { owner: 'microsoft', repo: 'vscode' },
          ],
        },
        { sessionId: 'test-session', signal: new AbortController().signal }
      );

      expect(mockLogToolCall).toHaveBeenCalledTimes(2);
      expect(mockLogToolCall).toHaveBeenNthCalledWith(
        1,
        GITHUB_SEARCH_CODE,
        ['facebook/react'],
        undefined,
        undefined,
        undefined
      );
      expect(mockLogToolCall).toHaveBeenNthCalledWith(
        2,
        GITHUB_SEARCH_CODE,
        ['microsoft/vscode'],
        undefined,
        undefined,
        undefined
      );
    });

    it('should not log when logging is disabled', async () => {
      const mockHandler = vi.fn().mockResolvedValue({
        content: [{ type: 'text' as const, text: 'success' }],
        isError: false,
      });

      mockIsLoggingEnabled.mockReturnValue(false);
      await withSecurityValidation(GITHUB_SEARCH_CODE, mockHandler)(
        { queries: [{ owner: 'facebook', repo: 'react' }] },
        { sessionId: 'test-session', signal: new AbortController().signal }
      );

      expect(mockLogToolCall).not.toHaveBeenCalled();
    });

    it('should log with an empty repo list when no repos are found in parameters', async () => {
      const mockHandler = vi.fn().mockResolvedValue({
        content: [{ type: 'text' as const, text: 'success' }],
        isError: false,
      });

      mockIsLoggingEnabled.mockReturnValue(true);
      await withSecurityValidation('test-tool', mockHandler)(
        { someOtherParam: 'value' },
        { sessionId: 'test-session', signal: new AbortController().signal }
      );

      expect(mockLogToolCall).toHaveBeenCalledWith(
        'test-tool',
        [],
        undefined,
        undefined,
        undefined
      );
    });

    it('should ignore logging errors and continue execution', async () => {
      const mockHandler = vi.fn().mockResolvedValue({
        content: [{ type: 'text' as const, text: 'success' }],
        isError: false,
      });

      mockIsLoggingEnabled.mockReturnValue(true);
      mockLogToolCall.mockRejectedValue(new Error('Logging failed'));

      const result = await withSecurityValidation(
        GITHUB_SEARCH_CODE,
        mockHandler
      )(
        { queries: [{ owner: 'facebook', repo: 'react' }] },
        { sessionId: 'test-session', signal: new AbortController().signal }
      );

      expect(result).toHaveProperty('content');
      expect(mockHandler).toHaveBeenCalled();
    });
  });

  describe('withSecurityValidation - SessionId Propagation', () => {
    it('should pass sessionId to handler', async () => {
      const mockHandler = vi.fn().mockResolvedValue({
        content: [{ type: 'text' as const, text: 'success' }],
        isError: false,
      });

      await withSecurityValidation('test-tool', mockHandler)(
        { query: 'test' },
        { sessionId: 'session-123', signal: new AbortController().signal }
      );

      expect(mockHandler).toHaveBeenCalledWith(
        { query: 'test' },
        undefined,
        'session-123'
      );
    });

    it('should pass authInfo and sessionId to handler', async () => {
      const mockHandler = vi.fn().mockResolvedValue({
        content: [{ type: 'text' as const, text: 'success' }],
        isError: false,
      });

      const mockAuthInfo = { token: 'test-token' };
      await withSecurityValidation('test-tool', mockHandler)(
        { query: 'test' },
        {
          authInfo: mockAuthInfo,
          sessionId: 'session-456',
          signal: new AbortController().signal,
        }
      );

      expect(mockHandler).toHaveBeenCalledWith(
        { query: 'test' },
        mockAuthInfo,
        'session-456'
      );
    });

    it('should handle undefined sessionId', async () => {
      const mockHandler = vi.fn().mockResolvedValue({
        content: [{ type: 'text' as const, text: 'success' }],
        isError: false,
      });

      await withSecurityValidation('test-tool', mockHandler)(
        { query: 'test' },
        { signal: new AbortController().signal }
      );

      expect(mockHandler).toHaveBeenCalledWith(
        { query: 'test' },
        undefined,
        undefined
      );
    });
  });

  describe('withSecurityValidation - Error Handling', () => {
    it('should return error when validation fails for dangerous key', async () => {
      const mockHandler = vi.fn();
      const wrappedHandler = withSecurityValidation('test-tool', mockHandler);
      const result = await wrappedHandler(
        { constructor: 'badkey' } as Record<string, unknown>,
        { sessionId: 'test', signal: new AbortController().signal }
      );

      expect(result.isError).toBe(true);
      const text = (result.content[0] as { text: string }).text;
      expect(text).toContain('Security validation failed');
      expect(text).toContain('Dangerous parameter key blocked');
      expect(mockHandler).not.toHaveBeenCalled();
    });

    it('should handle rejected handler without calling logSessionError', async () => {
      const mockHandler = vi
        .fn()
        .mockRejectedValue(new Error('handler failed'));
      const result = await withSecurityValidation('test-tool', mockHandler)(
        { query: 'clean' },
        { sessionId: 'test', signal: new AbortController().signal }
      );

      expect(result.isError).toBe(true);
      const text = (result.content[0] as { text: string }).text;
      expect(text).toContain('handler failed');
      await Promise.resolve();
      expect(mockLogSessionError).not.toHaveBeenCalled();
    });
  });

  describe('withSecurityValidation - Complex Parameter Extraction', () => {
    it('should extract repository from combined format', async () => {
      const mockHandler = vi.fn().mockResolvedValue({
        content: [{ type: 'text' as const, text: 'success' }],
        isError: false,
      });

      mockIsLoggingEnabled.mockReturnValue(true);
      await withSecurityValidation('test-tool', mockHandler)(
        {
          queries: [
            { repository: 'facebook/react' },
            { repository: 'microsoft/vscode' },
          ],
        },
        { sessionId: 'test', signal: new AbortController().signal }
      );

      expect(mockLogToolCall).toHaveBeenCalledTimes(2);
      expect(mockLogToolCall).toHaveBeenNthCalledWith(
        1,
        'test-tool',
        ['facebook/react'],
        undefined,
        undefined,
        undefined
      );
      expect(mockLogToolCall).toHaveBeenNthCalledWith(
        2,
        'test-tool',
        ['microsoft/vscode'],
        undefined,
        undefined,
        undefined
      );
    });

    it('should extract owner-only format', async () => {
      const mockHandler = vi.fn().mockResolvedValue({
        content: [{ type: 'text' as const, text: 'success' }],
        isError: false,
      });

      mockIsLoggingEnabled.mockReturnValue(true);
      await withSecurityValidation(GITHUB_SEARCH_REPOSITORIES, mockHandler)(
        { queries: [{ owner: 'facebook' }, { owner: 'microsoft' }] },
        { sessionId: 'test', signal: new AbortController().signal }
      );

      expect(mockLogToolCall).toHaveBeenCalledTimes(2);
      expect(mockLogToolCall).toHaveBeenNthCalledWith(
        1,
        GITHUB_SEARCH_REPOSITORIES,
        ['facebook'],
        undefined,
        undefined,
        undefined
      );
      expect(mockLogToolCall).toHaveBeenNthCalledWith(
        2,
        GITHUB_SEARCH_REPOSITORIES,
        ['microsoft'],
        undefined,
        undefined,
        undefined
      );
    });

    it('should extract from non-array parameters', async () => {
      const mockHandler = vi.fn().mockResolvedValue({
        content: [{ type: 'text' as const, text: 'success' }],
        isError: false,
      });

      mockIsLoggingEnabled.mockReturnValue(true);
      await withSecurityValidation('test-tool', mockHandler)(
        { owner: 'vercel', repo: 'next.js' },
        { sessionId: 'test', signal: new AbortController().signal }
      );

      expect(mockLogToolCall).toHaveBeenCalledWith(
        'test-tool',
        ['vercel/next.js'],
        undefined,
        undefined,
        undefined
      );
    });
  });

  describe('output passthrough — real sanitizer on clean content', () => {
    it('should return clean handler output unchanged', async () => {
      const handlerResult = {
        content: [{ type: 'text', text: 'clean output from handler' }],
      };
      const mockHandler = vi.fn().mockResolvedValue(handlerResult);

      const wrappedHandler = withSecurityValidation('test_tool', mockHandler);
      const result = await wrappedHandler({}, {});

      expect(result.content[0]?.text).toBe('clean output from handler');
    });

    it('should return clean output from withBasicSecurityValidation', async () => {
      const handlerResult = {
        content: [{ type: 'text', text: 'clean basic output' }],
      };
      const mockHandler = vi.fn().mockResolvedValue(handlerResult);

      const wrappedHandler = withBasicSecurityValidation(
        mockHandler,
        'local_tool'
      );
      const result = await wrappedHandler({});

      expect(result.content[0]?.text).toBe('clean basic output');
    });

    it('should preserve non-text content items without scanning', async () => {
      const mockHandler = vi.fn().mockResolvedValue({
        content: [
          { type: 'image', data: 'base64data', mimeType: 'image/png' },
          { type: 'text', text: 'safe text' },
        ],
      });

      const wrappedHandler = withSecurityValidation('test_tool', mockHandler);
      const result = await wrappedHandler({}, {});

      expect(result.content[0]).toMatchObject({ type: 'image' });
      expect(result.content[1]).toMatchObject({
        type: 'text',
        text: 'safe text',
      });
    });
  });
});
