import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  withSecurityValidation,
  configureSecurity,
} from '../src/withSecurityValidation.js';

// Mock injectable deps using hoisted
const mockLogToolCall = vi.hoisted(() => vi.fn().mockResolvedValue(undefined));
const mockIsLoggingEnabled = vi.hoisted(() => vi.fn(() => true));

// Mock content sanitizer
vi.mock('../src/contentSanitizer.js', () => ({
  ContentSanitizer: {
    validateInputParameters: vi.fn(() => ({
      isValid: true,
      sanitizedParams: {},
      warnings: [],
    })),
    sanitizeContent: vi.fn(content => ({
      content,
      hasSecrets: false,
      secretsDetected: [],
      warnings: [],
    })),
  },
}));

describe('withSecurityValidation logging', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // Reset mock to return resolved promise
    mockLogToolCall.mockResolvedValue(undefined);
    // Ensure logging is enabled for all tests
    mockIsLoggingEnabled.mockReturnValue(true);
    // Inject mocks via configureSecurity
    configureSecurity({
      logToolCall: mockLogToolCall,
      isLoggingEnabled: mockIsLoggingEnabled,
      isLocalTool: () => false,
    });
  });

  // Q3: reset global configureSecurity state after each test
  afterEach(() => {
    configureSecurity({
      logToolCall: undefined,
      logSessionError: undefined,
      isLoggingEnabled: undefined,
      isLocalTool: undefined,
    });
  });

  it('should extract repo and owner from bulk queries and log them', async () => {
    const mockHandler = vi.fn(async () => ({
      isError: false,
      content: [{ type: 'text' as const, text: 'success' }],
    }));

    const wrappedHandler = withSecurityValidation('test_tool', mockHandler);

    const args = {
      queries: [
        {
          id: 'query1',
          owner: 'test-owner',
          repo: 'test-repo',
          keywordsToSearch: ['test'],
        },
      ],
    };

    // Mock the sanitizer to return our test args
    const { ContentSanitizer } = await import('../src/contentSanitizer.js');
    vi.mocked(ContentSanitizer.validateInputParameters).mockReturnValue({
      isValid: true,
      sanitizedParams: args,
      warnings: [],
      hasSecrets: false,
    });

    await wrappedHandler(args, { signal: new AbortController().signal });

    expect(mockLogToolCall).toHaveBeenCalledWith(
      'test_tool',
      ['test-owner/test-repo'],
      undefined,
      undefined,
      undefined
    );
  });

  it('should extract repo and owner from direct params and log them', async () => {
    const mockHandler = vi.fn(async () => ({
      isError: false,
      content: [{ type: 'text' as const, text: 'success' }],
    }));

    const wrappedHandler = withSecurityValidation('test_tool', mockHandler);

    const args = {
      owner: 'direct-owner',
      repo: 'direct-repo',
      path: 'test/file.js',
    };

    // Mock the sanitizer to return our test args
    const { ContentSanitizer } = await import('../src/contentSanitizer.js');
    vi.mocked(ContentSanitizer.validateInputParameters).mockReturnValue({
      isValid: true,
      sanitizedParams: args,
      warnings: [],
      hasSecrets: false,
    });

    await wrappedHandler(args, { signal: new AbortController().signal });

    expect(mockLogToolCall).toHaveBeenCalledWith(
      'test_tool',
      ['direct-owner/direct-repo'],
      undefined,
      undefined,
      undefined
    );
  });

  it('should skip logging when logging is disabled', async () => {
    // Disable logging for this test
    mockIsLoggingEnabled.mockReturnValue(false);

    const mockHandler = vi.fn(async () => ({
      isError: false,
      content: [{ type: 'text' as const, text: 'success' }],
    }));

    const wrappedHandler = withSecurityValidation('test_tool', mockHandler);

    const args = {
      owner: 'test-owner',
      repo: 'test-repo',
    };

    // Mock the sanitizer
    const { ContentSanitizer } = await import('../src/contentSanitizer.js');
    vi.mocked(ContentSanitizer.validateInputParameters).mockReturnValue({
      isValid: true,
      sanitizedParams: args,
      warnings: [],
      hasSecrets: false,
    });

    await wrappedHandler(args, { signal: new AbortController().signal });

    // Verify isLoggingEnabled was called
    expect(mockIsLoggingEnabled).toHaveBeenCalled();

    // Verify logToolCall was NOT called since logging is disabled
    expect(mockLogToolCall).not.toHaveBeenCalled();

    // Verify the handler was still called successfully
    expect(mockHandler).toHaveBeenCalled();
  });

  it('should extract and log research fields from queries', async () => {
    const mockHandler = vi.fn(async () => ({
      isError: false,
      content: [{ type: 'text' as const, text: 'success' }],
    }));

    const wrappedHandler = withSecurityValidation('test_tool', mockHandler);

    const args = {
      queries: [
        {
          owner: 'test-owner',
          repo: 'test-repo',
          mainResearchGoal: 'Find authentication',
          researchGoal: 'Locate login function',
          reasoning: 'Need to understand auth flow',
        },
      ],
    };

    // Mock the sanitizer to return our test args
    const { ContentSanitizer } = await import('../src/contentSanitizer.js');
    vi.mocked(ContentSanitizer.validateInputParameters).mockReturnValue({
      isValid: true,
      sanitizedParams: args,
      warnings: [],
      hasSecrets: false,
    });

    await wrappedHandler(args, { signal: new AbortController().signal });

    expect(mockLogToolCall).toHaveBeenCalledWith(
      'test_tool',
      ['test-owner/test-repo'],
      'Find authentication',
      'Locate login function',
      'Need to understand auth flow'
    );
  });

  it('should extract and log partial research fields', async () => {
    const mockHandler = vi.fn(async () => ({
      isError: false,
      content: [{ type: 'text' as const, text: 'success' }],
    }));

    const wrappedHandler = withSecurityValidation('test_tool', mockHandler);

    const args = {
      queries: [
        {
          owner: 'test-owner',
          repo: 'test-repo',
          mainResearchGoal: 'Find authentication',
          reasoning: 'Need to understand auth flow',
        },
      ],
    };

    // Mock the sanitizer to return our test args
    const { ContentSanitizer } = await import('../src/contentSanitizer.js');
    vi.mocked(ContentSanitizer.validateInputParameters).mockReturnValue({
      isValid: true,
      sanitizedParams: args,
      warnings: [],
      hasSecrets: false,
    });

    await wrappedHandler(args, { signal: new AbortController().signal });

    expect(mockLogToolCall).toHaveBeenCalledWith(
      'test_tool',
      ['test-owner/test-repo'],
      'Find authentication',
      undefined,
      'Need to understand auth flow'
    );
  });

  describe('Bulk operations - individual query logging', () => {
    it('should log each query individually in bulk operations', async () => {
      const mockHandler = vi.fn(async () => ({
        isError: false,
        content: [{ type: 'text' as const, text: 'success' }],
      }));

      const wrappedHandler = withSecurityValidation('test_tool', mockHandler);

      const args = {
        queries: [
          {
            owner: 'owner1',
            repo: 'repo1',
            mainResearchGoal: 'Authentication',
            researchGoal: 'Find login',
            reasoning: 'Security audit',
          },
          {
            owner: 'owner2',
            repo: 'repo2',
            mainResearchGoal: 'Authorization',
            researchGoal: 'Find logout',
            reasoning: 'Access control',
          },
        ],
      };

      // Mock the sanitizer to return our test args
      const { ContentSanitizer } = await import('../src/contentSanitizer.js');
      vi.mocked(ContentSanitizer.validateInputParameters).mockReturnValue({
        isValid: true,
        sanitizedParams: args,
        warnings: [],
        hasSecrets: false,
      });

      await wrappedHandler(args, { signal: new AbortController().signal });

      // Verify logToolCall was called twice (once per query)
      expect(mockLogToolCall).toHaveBeenCalledTimes(2);

      // Verify first query was logged individually
      expect(mockLogToolCall).toHaveBeenNthCalledWith(
        1,
        'test_tool',
        ['owner1/repo1'],
        'Authentication',
        'Find login',
        'Security audit'
      );

      // Verify second query was logged individually
      expect(mockLogToolCall).toHaveBeenNthCalledWith(
        2,
        'test_tool',
        ['owner2/repo2'],
        'Authorization',
        'Find logout',
        'Access control'
      );
    });

    it('should log each query with its own repos in bulk operations', async () => {
      const mockHandler = vi.fn(async () => ({
        isError: false,
        content: [{ type: 'text' as const, text: 'success' }],
      }));

      const wrappedHandler = withSecurityValidation('test_tool', mockHandler);

      const args = {
        queries: [
          {
            repository: 'facebook/react',
            mainResearchGoal: 'React hooks',
          },
          {
            owner: 'microsoft',
            repo: 'vscode',
            mainResearchGoal: 'VS Code API',
          },
          {
            owner: 'vercel',
            mainResearchGoal: 'Vercel tools',
          },
        ],
      };

      const { ContentSanitizer } = await import('../src/contentSanitizer.js');
      vi.mocked(ContentSanitizer.validateInputParameters).mockReturnValue({
        isValid: true,
        sanitizedParams: args,
        warnings: [],
        hasSecrets: false,
      });

      await wrappedHandler(args, { signal: new AbortController().signal });

      // Verify logToolCall was called 3 times (once per query)
      expect(mockLogToolCall).toHaveBeenCalledTimes(3);

      // Verify each query logged with its specific repo
      expect(mockLogToolCall).toHaveBeenNthCalledWith(
        1,
        'test_tool',
        ['facebook/react'],
        'React hooks',
        undefined,
        undefined
      );

      expect(mockLogToolCall).toHaveBeenNthCalledWith(
        2,
        'test_tool',
        ['microsoft/vscode'],
        'VS Code API',
        undefined,
        undefined
      );

      expect(mockLogToolCall).toHaveBeenNthCalledWith(
        3,
        'test_tool',
        ['vercel'],
        'Vercel tools',
        undefined,
        undefined
      );
    });

    it('should skip logging queries without repos in bulk operations', async () => {
      const mockHandler = vi.fn(async () => ({
        isError: false,
        content: [{ type: 'text' as const, text: 'success' }],
      }));

      const wrappedHandler = withSecurityValidation('test_tool', mockHandler);

      const args = {
        queries: [
          {
            owner: 'owner1',
            repo: 'repo1',
            keywordsToSearch: ['test'],
          },
          {
            keywordsToSearch: ['test2'],
            // No repo info
          },
          {
            owner: 'owner3',
            repo: 'repo3',
            keywordsToSearch: ['test3'],
          },
        ],
      };

      const { ContentSanitizer } = await import('../src/contentSanitizer.js');
      vi.mocked(ContentSanitizer.validateInputParameters).mockReturnValue({
        isValid: true,
        sanitizedParams: args,
        warnings: [],
        hasSecrets: false,
      });

      await wrappedHandler(args, { signal: new AbortController().signal });

      // Only queries with repos should be logged (2 out of 3)
      expect(mockLogToolCall).toHaveBeenCalledTimes(2);

      expect(mockLogToolCall).toHaveBeenNthCalledWith(
        1,
        'test_tool',
        ['owner1/repo1'],
        undefined,
        undefined,
        undefined
      );

      expect(mockLogToolCall).toHaveBeenNthCalledWith(
        2,
        'test_tool',
        ['owner3/repo3'],
        undefined,
        undefined,
        undefined
      );
    });

    it('should handle bulk operations with mixed research fields', async () => {
      const mockHandler = vi.fn(async () => ({
        isError: false,
        content: [{ type: 'text' as const, text: 'success' }],
      }));

      const wrappedHandler = withSecurityValidation('test_tool', mockHandler);

      const args = {
        queries: [
          {
            owner: 'owner1',
            repo: 'repo1',
            mainResearchGoal: 'Goal 1',
            researchGoal: 'Subgoal 1',
            reasoning: 'Reason 1',
          },
          {
            owner: 'owner2',
            repo: 'repo2',
            mainResearchGoal: 'Goal 2',
            // Missing researchGoal and reasoning
          },
          {
            owner: 'owner3',
            repo: 'repo3',
            // Only reasoning, no goals
            reasoning: 'Reason 3',
          },
        ],
      };

      const { ContentSanitizer } = await import('../src/contentSanitizer.js');
      vi.mocked(ContentSanitizer.validateInputParameters).mockReturnValue({
        isValid: true,
        sanitizedParams: args,
        warnings: [],
        hasSecrets: false,
      });

      await wrappedHandler(args, { signal: new AbortController().signal });

      expect(mockLogToolCall).toHaveBeenCalledTimes(3);

      expect(mockLogToolCall).toHaveBeenNthCalledWith(
        1,
        'test_tool',
        ['owner1/repo1'],
        'Goal 1',
        'Subgoal 1',
        'Reason 1'
      );

      expect(mockLogToolCall).toHaveBeenNthCalledWith(
        2,
        'test_tool',
        ['owner2/repo2'],
        'Goal 2',
        undefined,
        undefined
      );

      expect(mockLogToolCall).toHaveBeenNthCalledWith(
        3,
        'test_tool',
        ['owner3/repo3'],
        undefined,
        undefined,
        'Reason 3'
      );
    });
  });

  it('should log without research fields when none are provided', async () => {
    const mockHandler = vi.fn(async () => ({
      isError: false,
      content: [{ type: 'text' as const, text: 'success' }],
    }));

    const wrappedHandler = withSecurityValidation('test_tool', mockHandler);

    const args = {
      queries: [
        {
          owner: 'test-owner',
          repo: 'test-repo',
          keywordsToSearch: ['test'],
        },
      ],
    };

    // Mock the sanitizer to return our test args
    const { ContentSanitizer } = await import('../src/contentSanitizer.js');
    vi.mocked(ContentSanitizer.validateInputParameters).mockReturnValue({
      isValid: true,
      sanitizedParams: args,
      warnings: [],
      hasSecrets: false,
    });

    await wrappedHandler(args, { signal: new AbortController().signal });

    expect(mockLogToolCall).toHaveBeenCalledWith(
      'test_tool',
      ['test-owner/test-repo'],
      undefined,
      undefined,
      undefined
    );
  });
});
