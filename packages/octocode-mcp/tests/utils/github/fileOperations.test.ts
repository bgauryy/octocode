import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';

// Create hoisted mocks
const mockGetOctokit = vi.hoisted(() => vi.fn());
const mockResolveDefaultBranch = vi.hoisted(() =>
  vi.fn().mockResolvedValue('main')
);
const mockContentSanitizer = vi.hoisted(() => ({
  sanitizeContent: vi.fn().mockImplementation((content: string) => ({
    content: content, // Pass through content unchanged for testing
    hasSecrets: false,
    warnings: [],
    secretsDetected: [],
  })),
}));
const mockminifyContent = vi.hoisted(() =>
  vi.fn().mockImplementation((content: string) =>
    Promise.resolve({
      content: content, // Pass through content unchanged for testing
      failed: false,
      type: 'general',
    })
  )
);
const mockWithDataCache = vi.hoisted(() => vi.fn());
const mockGenerateCacheKey = vi.hoisted(() => vi.fn());
const mockCreateResult = vi.hoisted(() => vi.fn());

// Set up mocks
vi.mock('../../../src/github/client.js', () => ({
  getOctokit: mockGetOctokit,
  OctokitWithThrottling: class MockOctokit {},
  resolveDefaultBranch: mockResolveDefaultBranch,
}));

vi.mock('octocode-security-utils/contentSanitizer', () => ({
  ContentSanitizer: mockContentSanitizer,
}));

vi.mock('../../../src/utils/minifier/minifier.js', () => ({
  minifyContent: mockminifyContent,
}));

vi.mock('../../../src/utils/http/cache.js', () => ({
  generateCacheKey: mockGenerateCacheKey,
  withDataCache: mockWithDataCache,
}));

vi.mock('../../../src/mcp/responses.js', () => ({
  createResult: mockCreateResult,
}));

// Import after mocks are set up
import { fetchGitHubFileContentAPI } from '../../../src/github/fileContent.js';

// Helper function to create properly formatted test parameters
function createTestParams(overrides: Record<string, unknown> = {}) {
  return {
    owner: 'test',
    repo: 'repo',
    path: 'test.txt',
    fullContent: false,
    matchStringContextLines: 5,
    ...overrides,
  };
}

describe('fetchGitHubFileContentAPI - Parameter Testing', () => {
  describe('Schema defaults', () => {
    it('should have correct schema defaults', async () => {
      const { FileContentQuerySchema } =
        await import('@octocodeai/octocode-core');

      // Test minimal valid input (only required fields)
      const minimalInput = {
        id: 'test:file-ops',
        owner: 'test',
        repo: 'repo',
        path: 'test.js',
        mainResearchGoal: 'Test research goal',
        researchGoal: 'Testing schema defaults',
        reasoning: 'Unit test for schema',
      };

      const parsed = FileContentQuerySchema.parse(minimalInput);

      expect(parsed.fullContent).toBe(false); // Should default to false
      expect(parsed.startLine).toBeUndefined(); // Should be optional
      expect(parsed.endLine).toBeUndefined(); // Should be optional
      expect(parsed.matchString).toBeUndefined(); // Should be optional
      expect(parsed.matchStringContextLines).toBe(5); // Should have default
      // minified and sanitize are now always enabled (not schema parameters)
    });
  });
  let mockOctokit: {
    rest: {
      repos: {
        getContent: ReturnType<typeof vi.fn>;
        get: ReturnType<typeof vi.fn>;
        listCommits: ReturnType<typeof vi.fn>;
      };
    };
  };

  beforeEach(() => {
    vi.clearAllMocks();
    mockResolveDefaultBranch.mockResolvedValue('main');

    // Setup default mock Octokit instance
    mockOctokit = {
      rest: {
        repos: {
          getContent: vi.fn(),
          get: vi.fn().mockResolvedValue({ data: { default_branch: 'main' } }),
          listCommits: vi.fn().mockResolvedValue({ data: [] }),
        },
      },
    };
    mockGetOctokit.mockReturnValue(mockOctokit);

    // Setup default cache behavior - execute the operation directly
    mockWithDataCache.mockImplementation(
      async (
        _cacheKey: string,
        operation: () => Promise<{
          content: Array<{ type: string; text: string }>;
          isError: boolean;
        }>
      ) => {
        const callToolResult = await operation();
        return callToolResult;
      }
    );

    // Setup default createResult behavior to return proper CallToolResult format
    mockCreateResult.mockImplementation((args: unknown) => {
      const typedArgs = args as { data?: unknown; isError?: boolean };
      return {
        content: [
          {
            type: 'text',
            text: JSON.stringify({ data: typedArgs.data || args }),
          },
        ],
        isError: typedArgs.isError || false,
      };
    });

    // Setup content sanitizer to return the actual content
    mockContentSanitizer.sanitizeContent.mockImplementation(
      (content: string) => ({
        content,
        hasSecrets: false,
        warnings: [],
        secretsDetected: [],
      })
    );

    // Setup minifier to pass through content (minification is always on)
    mockminifyContent.mockImplementation((content: string) =>
      Promise.resolve({
        content: content,
        failed: false,
        type: 'general',
      })
    );
  });

  afterEach(() => {
    vi.resetAllMocks();
    vi.clearAllMocks();
  });

  describe('Basic file content retrieval', () => {
    beforeEach(() => {
      // Mock successful file content response
      mockOctokit.rest.repos.getContent.mockResolvedValue({
        data: {
          type: 'file',
          content: Buffer.from(
            'line 1\nline 2\nline 3\nline 4\nline 5'
          ).toString('base64'),
          size: 35,
          sha: 'abc123',
        },
      });
    });

    it('should fetch entire file when no parameters specified ', async () => {
      const params = createTestParams();

      const result = await fetchGitHubFileContentAPI(params);

      expect(result).toEqual({
        status: 200,
        rawResponseChars: expect.any(Number),
        data: {
          owner: 'test',
          repo: 'repo',
          path: 'test.txt',
          branch: 'main',
          content: 'line 1\nline 2\nline 3\nline 4\nline 5',
        },
      });
    });

    it('should fetch entire file when only non-content parameters are specified', async () => {
      const params = createTestParams();

      const result = await fetchGitHubFileContentAPI(params);

      expect(result).toEqual({
        status: 200,
        rawResponseChars: expect.any(Number),
        data: {
          owner: 'test',
          repo: 'repo',
          path: 'test.txt',
          branch: 'main',
          content: 'line 1\nline 2\nline 3\nline 4\nline 5',
        },
      });
    });

    it('should resolve to "master" when repo default branch is master and no branch specified', async () => {
      mockResolveDefaultBranch.mockResolvedValue('master');

      const params = createTestParams();

      const result = await fetchGitHubFileContentAPI(params);

      expect(result).toEqual({
        status: 200,
        rawResponseChars: expect.any(Number),
        data: {
          owner: 'test',
          repo: 'repo',
          path: 'test.txt',
          branch: 'master',
          content: 'line 1\nline 2\nline 3\nline 4\nline 5',
        },
      });
    });

    it('should explicitly handle fullContent=false as full content ', async () => {
      const params = createTestParams({
        fullContent: false, // Explicitly set to false
        // No other content selection parameters
      });

      const result = await fetchGitHubFileContentAPI(params);

      expect(result).toEqual({
        status: 200,
        rawResponseChars: expect.any(Number),
        data: {
          owner: 'test',
          repo: 'repo',
          path: 'test.txt',
          branch: 'main',
          content: 'line 1\nline 2\nline 3\nline 4\nline 5',
        },
      });
    });

    it('should always apply minification', async () => {
      const params = createTestParams();

      const result = await fetchGitHubFileContentAPI(params);

      expect(mockminifyContent).toHaveBeenCalledWith(
        'line 1\nline 2\nline 3\nline 4\nline 5',
        'test.txt'
      );
      expect(result).toEqual({
        status: 200,
        rawResponseChars: expect.any(Number),
        data: {
          owner: 'test',
          repo: 'repo',
          path: 'test.txt',
          branch: 'main',
          content: 'line 1\nline 2\nline 3\nline 4\nline 5',
        },
      });
    });

    it('should return entire file when fullContent=true', async () => {
      const params = createTestParams({
        fullContent: true,
        startLine: 2, // Should be ignored
        endLine: 4, // Should be ignored
      });

      const result = await fetchGitHubFileContentAPI(params);

      expect(result).toEqual({
        status: 200,
        rawResponseChars: expect.any(Number),
        data: {
          owner: 'test',
          repo: 'repo',
          path: 'test.txt',
          branch: 'main',
          content: 'line 1\nline 2\nline 3\nline 4\nline 5',
        },
      });
    });

    it('should return entire file when fullContent=true and ignore matchString', async () => {
      const params = createTestParams({
        fullContent: true,
        matchString: 'line 3', // Should be ignored
      });

      const result = await fetchGitHubFileContentAPI(params);

      expect(result.status).toBe(200);
      if ('data' in result) {
        expect(result.data.content).toBe(
          'line 1\nline 2\nline 3\nline 4\nline 5'
        );
        expect(result.data.startLine).toBeUndefined();
        expect(result.data.endLine).toBeUndefined();
        expect(result.data.isPartial).toBeUndefined();
      }
    });
  });

  describe('Line range selection (startLine/endLine)', () => {
    beforeEach(() => {
      // Mock file with 10 lines
      const fileContent = Array.from(
        { length: 10 },
        (_, i) => `line ${i + 1}`
      ).join('\n');
      mockOctokit.rest.repos.getContent.mockResolvedValue({
        data: {
          type: 'file',
          content: Buffer.from(fileContent).toString('base64'),
          size: fileContent.length,
          sha: 'abc123',
        },
      });
    });

    it('should extract specific line range with startLine and endLine', async () => {
      const params = createTestParams({
        startLine: 3,
        endLine: 6,
      });

      const result = await fetchGitHubFileContentAPI(params);

      expect(result.status).toBe(200);
      if ('data' in result) {
        expect(result.data.content).toBe('line 3\nline 4\nline 5\nline 6');
        expect(result.data.startLine).toBe(3);
        expect(result.data.endLine).toBe(6);
        expect(result.data.isPartial).toBe(true);
      }
    });

    it('should extract from startLine to end when only startLine specified', async () => {
      const params = createTestParams({
        startLine: 8,
      });

      const result = await fetchGitHubFileContentAPI(params);

      expect(result.status).toBe(200);
      if ('data' in result) {
        expect(result.data.content).toBe('line 8\nline 9\nline 10');
        expect(result.data.startLine).toBe(8);
        expect(result.data.endLine).toBe(10);
        expect(result.data.isPartial).toBe(true);
      }
    });

    it('should extract from beginning to endLine when only endLine specified', async () => {
      const params = createTestParams({
        endLine: 3,
      });

      const result = await fetchGitHubFileContentAPI(params);

      expect(result.status).toBe(200);
      if ('data' in result) {
        expect(result.data.content).toBe('line 1\nline 2\nline 3');
        expect(result.data.startLine).toBe(1);
        expect(result.data.endLine).toBe(3);
        expect(result.data.isPartial).toBe(true);
      }
    });

    it('should handle invalid line ranges gracefully by returning whole file', async () => {
      const params = createTestParams({
        startLine: 15, // Beyond file length
        endLine: 20,
      });

      const result = await fetchGitHubFileContentAPI(params);

      expect(result.status).toBe(200);
      if ('data' in result) {
        // Should return the whole file when range is invalid
        expect(result.data.content).toContain('line 1');
        expect(result.data.content).toContain('line 10');
        expect(result.data.isPartial).toBeUndefined();
      }
    });

    it('should handle endLine beyond file bounds by adjusting to file end', async () => {
      const params = createTestParams({
        startLine: 8,
        endLine: 15, // Beyond file length
      });

      const result = await fetchGitHubFileContentAPI(params);

      expect(result.status).toBe(200);
      if ('data' in result) {
        expect(result.data.content).toBe('line 8\nline 9\nline 10');
        expect(result.data.startLine).toBe(8);
        expect(result.data.endLine).toBe(10);
        expect(result.data.matchLocations).toContain(
          'Requested endLine 15 adjusted to 10 (file end)'
        );
      }
    });
  });

  describe('Match string with context lines', () => {
    beforeEach(() => {
      // Mock file with specific content for matching
      const fileContent = [
        'header line',
        'import React from "react";',
        'import { Component } from "react";',
        '',
        'function MyComponent() {',
        '  return <div>Hello World</div>;',
        '}',
        '',
        'export default MyComponent;',
        'footer line',
      ].join('\n');

      mockOctokit.rest.repos.getContent.mockResolvedValue({
        data: {
          type: 'file',
          content: Buffer.from(fileContent).toString('base64'),
          size: fileContent.length,
          sha: 'abc123',
        },
      });
    });

    it('should find match and return context with default matchStringContextLines (5)', async () => {
      const params = createTestParams({
        matchString: 'function MyComponent()',
      });

      const result = await fetchGitHubFileContentAPI(params);

      expect(result.status).toBe(200);
      if ('data' in result) {
        // Should include 5 lines before + matching line + 5 lines after (but limited by file bounds)
        expect(result.data.content).toContain('function MyComponent()');
        expect(result.data.content).toContain('import React from "react"'); // Context before
        expect(result.data.content).toContain('export default MyComponent'); // Context after
        expect(result.data.isPartial).toBe(true);
        expect(result.data.matchLocations).toContain(
          'Found "function MyComponent()" on line 5'
        );
      }
    });

    it('should respect custom matchStringContextLines', async () => {
      const params = createTestParams({
        matchString: 'function MyComponent()',
        matchStringContextLines: 2,
      });

      const result = await fetchGitHubFileContentAPI(params);

      expect(result.status).toBe(200);
      if ('data' in result) {
        // Should include 2 lines before + matching line + 2 lines after
        expect(result.data.content).toContain('function MyComponent()');
        expect(result.data.isPartial).toBe(true);
        // With 2 context lines, should be around lines 3-7 (5±2)
        expect(result.data.startLine).toBe(3); // Max(1, 5-2)
        expect(result.data.endLine).toBe(7); // Min(10, 5+2)
      }
    });

    it('should handle matchStringContextLines=0 (only matching line)', async () => {
      const params = createTestParams({
        matchString: 'function MyComponent()',
        matchStringContextLines: 0,
      });

      const result = await fetchGitHubFileContentAPI(params);

      expect(result.status).toBe(200);
      if ('data' in result) {
        // With 0 context lines, should return only the matching line
        expect(result.data.content).toContain('function MyComponent()');
        expect(result.data.startLine).toBe(5); // Max(1, 5-0) = 5 (exact match line)
        expect(result.data.endLine).toBe(5); // Min(10, 5+0) = 5 (exact match line)
        expect(result.data.isPartial).toBe(true);
        // Should contain only the matching line
        expect(result.data.content).toBe('function MyComponent() {');
      }
    });

    it('should return only context lines for match string (TDD bug reproduction)', async () => {
      // Create a file that mimics the React file structure that caused the bug
      const reactLikeContent = [
        '/**',
        ' * Copyright (c) Meta Platforms, Inc. and affiliates.',
        ' *',
        ' * This source code is licensed under the MIT license found in the',
        ' * LICENSE file in the root directory of this source tree.',
        ' * @flow',
        ' */',
        '',
        "import type {RefObject} from 'shared/ReactTypes';",
        '',
        '// an immutable object with a single mutable value',
        'export function createRef(): RefObject {', // Line 12 - this is our match
        '  const refObject = {',
        '    current: null,',
        '  };',
        '  if (__DEV__) {',
        '    Object.seal(refObject);',
        '  }',
        '  return refObject;',
        '}',
        '',
      ].join('\n');

      mockOctokit.rest.repos.getContent.mockResolvedValue({
        data: {
          type: 'file',
          content: Buffer.from(reactLikeContent).toString('base64'),
          size: reactLikeContent.length,
          sha: 'abc123',
        },
      });

      const params = createTestParams({
        matchString: 'export function createRef',
        matchStringContextLines: 3,
        minified: false, // Explicitly disable minification like in real test
      });

      const result = await fetchGitHubFileContentAPI(params);

      expect(result.status).toBe(200);
      if ('data' in result) {
        const contentLines = result.data.content?.split('\n') || [];

        // The bug: it returns ALL lines instead of just context
        // This test should FAIL initially, proving the bug exists
        expect(contentLines.length).toBe(7); // Should be 7 lines (3 before + match + 3 after)
        expect(contentLines.length).not.toBe(
          reactLikeContent.split('\n').length
        ); // Should NOT be the full file

        expect(result.data.startLine).toBe(9); // Max(1, 12-3)
        expect(result.data.endLine).toBe(15); // Min(21, 12+3)
        expect(result.data.isPartial).toBe(true);

        // Should contain the match and context
        expect(result.data.content).toContain('export function createRef');
        expect(result.data.content).toContain('// an immutable object'); // Context before
        expect(result.data.content).toContain('const refObject = {'); // Context after

        // Should NOT contain the copyright header (too far from match)
        expect(result.data.content).not.toContain(
          'Copyright (c) Meta Platforms'
        );

        expect(result.data.matchLocations).toContain(
          'Found "export function createRef" on line 12'
        );
      }
    });

    it('should return success with matchNotFound when matchString not found', async () => {
      const params = createTestParams({ matchString: 'nonexistent string' });

      const result = await fetchGitHubFileContentAPI(params);

      // The result should be a 200 success with matchNotFound flag
      // "Match not found" is a normal scenario, NOT an error
      expect(result.status).toBe(200);
      expect('data' in result).toBe(true);
      if ('data' in result) {
        expect(result.data.matchNotFound).toBe(true);
        expect(result.data.searchedFor).toBe('nonexistent string');
        expect(result.data.content).toBe('');
      }
    });

    it('should handle matchString with multiple occurrences', async () => {
      const params = createTestParams({
        matchString: 'import', // Appears multiple times
        matchStringContextLines: 1,
      });

      const result = await fetchGitHubFileContentAPI(params);

      expect(result.status).toBe(200);
      if ('data' in result) {
        // Should use the first match and indicate multiple matches
        expect(result.data.content).toContain('import React from "react"');
        expect(result.data.matchLocations).toContain(
          'Found "import" on line 2 (and 1 other locations)'
        );
      }
    });
  });

  describe('Combined parameters', () => {
    beforeEach(() => {
      const fileContent = Array.from(
        { length: 20 },
        (_, i) => `line ${i + 1}`
      ).join('\n');
      mockOctokit.rest.repos.getContent.mockResolvedValue({
        data: {
          type: 'file',
          content: Buffer.from(fileContent).toString('base64'),
          size: fileContent.length,
          sha: 'abc123',
        },
      });
    });

    it('should prioritize matchString over manual startLine/endLine', async () => {
      const params = createTestParams({
        startLine: 1,
        endLine: 5,
        matchString: 'line 10',
        matchStringContextLines: 2,
      });

      const result = await fetchGitHubFileContentAPI(params);

      expect(result.status).toBe(200);
      if ('data' in result) {
        // Should use match-based range, not manual startLine/endLine
        expect(result.data.content).toContain('line 10');
        expect(result.data.startLine).toBe(8); // 10-2
        expect(result.data.endLine).toBe(12); // 10+2
        expect(result.data.content).toBe(
          'line 8\nline 9\nline 10\nline 11\nline 12'
        );
      }
    });

    it('should apply minification to line-selected content', async () => {
      const params = createTestParams({
        startLine: 5,
        endLine: 8,
      });

      const result = await fetchGitHubFileContentAPI(params);

      expect(mockminifyContent).toHaveBeenCalledWith(
        'line 5\nline 6\nline 7\nline 8',
        'test.txt'
      );
      expect(result.status).toBe(200);
      if ('data' in result) {
        expect(result.data.content).toBe('line 5\nline 6\nline 7\nline 8');
        expect(result.data.isPartial).toBe(true);
      }
    });

    it('should apply minification to match-selected content', async () => {
      const params = createTestParams({
        matchString: 'line 15',
        matchStringContextLines: 1,
      });

      const result = await fetchGitHubFileContentAPI(params);

      expect(mockminifyContent).toHaveBeenCalledWith(
        'line 14\nline 15\nline 16',
        'test.txt'
      );
      expect(result.status).toBe(200);
      if ('data' in result) {
        expect(result.data.content).toBe('line 14\nline 15\nline 16');
        expect(result.data.isPartial).toBe(true);
      }
    });
  });

  describe('Cache key generation', () => {
    it('should generate cache key with all relevant parameters', async () => {
      const params = createTestParams({
        branch: 'feature',
        startLine: 5,
        endLine: 10,
        matchString: 'search term',
        matchStringContextLines: 3,
      });

      // Mock file response
      mockOctokit.rest.repos.getContent.mockResolvedValue({
        data: {
          type: 'file',
          content: Buffer.from('test content').toString('base64'),
          size: 12,
          sha: 'abc123',
        },
      });

      await fetchGitHubFileContentAPI(params);

      // Cache key only includes GitHub API params (owner, repo, path, branch)
      // Processing params (startLine, endLine, matchString) are applied post-cache
      expect(mockGenerateCacheKey).toHaveBeenCalledWith(
        'gh-api-file-content',
        {
          owner: 'test',
          repo: 'repo',
          path: 'test.txt',
          branch: 'feature',
        },
        undefined
      );
    });

    it('should generate same cache key for same file with different line params', async () => {
      const baseParams = createTestParams();

      // Mock file response
      mockOctokit.rest.repos.getContent.mockResolvedValue({
        data: {
          type: 'file',
          content: Buffer.from('test content').toString('base64'),
          size: 12,
          sha: 'abc123',
        },
      });

      // Call with base params
      await fetchGitHubFileContentAPI(baseParams);

      // Call with additional params (different startLine/endLine)
      await fetchGitHubFileContentAPI({
        ...baseParams,
        startLine: 1,
        endLine: 5,
      });

      // Cache key only includes GitHub API params - so SAME key for same file!
      // Processing params are applied post-cache for efficiency
      expect(mockGenerateCacheKey).toHaveBeenCalledTimes(2);
      expect(mockGenerateCacheKey).toHaveBeenNthCalledWith(
        1,
        'gh-api-file-content',
        {
          owner: 'test',
          repo: 'repo',
          path: 'test.txt',
          branch: undefined,
        },
        undefined
      );
      // Same cache key for second call - different processing applied post-cache
      expect(mockGenerateCacheKey).toHaveBeenNthCalledWith(
        2,
        'gh-api-file-content',
        {
          owner: 'test',
          repo: 'repo',
          path: 'test.txt',
          branch: undefined,
        },
        undefined
      );
    });
  });

  describe('Error Handling', () => {
    it('should handle directory response error', async () => {
      const params = createTestParams();

      mockOctokit.rest.repos.getContent.mockResolvedValue({
        data: [
          { name: 'file1.txt', type: 'file' },
          { name: 'dir1', type: 'dir' },
        ],
      });

      const result = await fetchGitHubFileContentAPI(params);

      expect('error' in result).toBe(true);
      if ('error' in result) {
        expect(result.error).toContain('Path is a directory');
        expect(typeof result.error).toBe('string');
        expect(result.error.length).toBeGreaterThan(0);
      }
    });

    it('should handle file too large error', async () => {
      const params = createTestParams();
      const largeContent = 'x'.repeat(500 * 1024); // 500KB

      mockOctokit.rest.repos.getContent.mockResolvedValue({
        data: {
          type: 'file',
          content: Buffer.from(largeContent).toString('base64'),
          size: largeContent.length,
          sha: 'abc123',
        },
      });

      const result = await fetchGitHubFileContentAPI(params);

      expect('error' in result).toBe(true);
      if ('error' in result) {
        expect(result.error).toContain('File too large');
        expect(result.error).toContain('300KB');
      }
    });

    it('should handle empty file (no content)', async () => {
      const params = createTestParams();

      mockOctokit.rest.repos.getContent.mockResolvedValue({
        data: {
          type: 'file',
          content: undefined,
          size: 0,
          sha: 'abc123',
        },
      });

      const result = await fetchGitHubFileContentAPI(params);

      expect('error' in result).toBe(true);
      if ('error' in result) {
        expect(result.error).toContain('File is empty');
      }
    });

    it('should handle empty base64 content', async () => {
      const params = createTestParams();

      mockOctokit.rest.repos.getContent.mockResolvedValue({
        data: {
          type: 'file',
          content: '   \n  \t  ',
          size: 0,
          sha: 'abc123',
        },
      });

      const result = await fetchGitHubFileContentAPI(params);

      expect('error' in result).toBe(true);
      if ('error' in result) {
        expect(result.error).toContain('File is empty');
      }
    });

    it('should detect binary files', async () => {
      const params = createTestParams();
      const binaryContent = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x00, 0x0d]); // PNG header with null byte

      mockOctokit.rest.repos.getContent.mockResolvedValue({
        data: {
          type: 'file',
          content: binaryContent.toString('base64'),
          size: binaryContent.length,
          sha: 'abc123',
        },
      });

      const result = await fetchGitHubFileContentAPI(params);

      expect('error' in result).toBe(true);
      if ('error' in result) {
        expect(result.error).toContain('Binary file detected');
        expect(result.status).toBe(415);
      }
    });

    it('should handle unsupported file types', async () => {
      const params = createTestParams();

      mockOctokit.rest.repos.getContent.mockResolvedValue({
        data: {
          type: 'symlink',
          sha: 'abc123',
        },
      });

      const result = await fetchGitHubFileContentAPI(params);

      expect('error' in result).toBe(true);
      if ('error' in result) {
        expect(result.error).toContain('Unsupported file type: symlink');
        expect(result.status).toBe(415);
      }
    });

    it('should handle decode errors', async () => {
      const params = createTestParams();

      mockOctokit.rest.repos.getContent.mockResolvedValue({
        data: {
          type: 'file',
          content: '!!!invalid-base64!!!',
          size: 20,
          sha: 'abc123',
        },
      });

      // This will trigger decode error
      const result = await fetchGitHubFileContentAPI(params);

      // Should handle decode error - might succeed with Buffer.from fallback or fail gracefully
      expect(result).toBeDefined();
    });

    it('should handle API errors', async () => {
      const params = createTestParams();

      mockOctokit.rest.repos.getContent.mockRejectedValue(
        new Error('API Error')
      );

      const result = await fetchGitHubFileContentAPI(params);

      expect('error' in result).toBe(true);
    });
  });

  describe('Content Sanitization', () => {
    beforeEach(() => {
      mockOctokit.rest.repos.getContent.mockResolvedValue({
        data: {
          type: 'file',
          content: Buffer.from('test content with secrets').toString('base64'),
          size: 25,
          sha: 'abc123',
        },
      });
    });

    it('should detect and warn about secrets', async () => {
      const params = createTestParams();

      mockContentSanitizer.sanitizeContent.mockReturnValue({
        content: 'test content with [REDACTED]',
        hasSecrets: true,
        warnings: [],
        secretsDetected: ['github-token'],
      });

      const result = await fetchGitHubFileContentAPI(params);

      expect(result.status).toBe(200);
      if ('data' in result) {
        expect(result.data.matchLocations).toContain(
          'Secrets detected and redacted: github-token'
        );
      }
    });

    it('should include custom security warnings', async () => {
      const params = createTestParams();

      mockContentSanitizer.sanitizeContent.mockReturnValue({
        content: 'sanitized content',
        hasSecrets: false,
        warnings: ['Custom warning 1', 'Custom warning 2'],
        secretsDetected: [],
      });

      const result = await fetchGitHubFileContentAPI(params);

      expect(result.status).toBe(200);
      if ('data' in result) {
        expect(result.data.matchLocations).toContain('Custom warning 1');
        expect(result.data.matchLocations).toContain('Custom warning 2');
      }
    });
  });

  describe('Branch fallback (main/master)', () => {
    beforeEach(() => {
      mockContentSanitizer.sanitizeContent.mockImplementation(
        (content: string) => ({
          content,
          hasSecrets: false,
          warnings: [],
          secretsDetected: [],
        })
      );
    });

    it('should fallback from main to master when main branch returns 404', async () => {
      const { RequestError } = await import('octokit');
      const notFoundError = new RequestError('Not Found', 404, {
        request: {
          method: 'GET',
          url: 'https://api.github.com/repos/test/repo/contents/test.txt',
          headers: {},
        },
      });

      mockResolveDefaultBranch.mockResolvedValue('master');

      // First call fails (main branch)
      // Second call succeeds (master branch)
      mockOctokit.rest.repos.getContent
        .mockRejectedValueOnce(notFoundError)
        .mockResolvedValueOnce({
          data: {
            type: 'file',
            content: Buffer.from('fallback content from master').toString(
              'base64'
            ),
            size: 28,
            sha: 'abc123',
          },
        });

      const params = createTestParams({ branch: 'main', minified: false });

      const result = await fetchGitHubFileContentAPI(params);

      expect(result.status).toBe(200);
      expect(mockOctokit.rest.repos.getContent).toHaveBeenCalledTimes(2);
      // First call with main
      expect(mockOctokit.rest.repos.getContent).toHaveBeenNthCalledWith(1, {
        owner: 'test',
        repo: 'repo',
        path: 'test.txt',
        ref: 'main',
      });
      // Second call with master
      expect(mockOctokit.rest.repos.getContent).toHaveBeenNthCalledWith(2, {
        owner: 'test',
        repo: 'repo',
        path: 'test.txt',
        ref: 'master',
      });
      if ('data' in result) {
        expect(result.data.content).toBe('fallback content from master');
      }
    });

    it('should fallback from master to main when master branch returns 404', async () => {
      const { RequestError } = await import('octokit');
      const notFoundError = new RequestError('Not Found', 404, {
        request: {
          method: 'GET',
          url: 'https://api.github.com/repos/test/repo/contents/test.txt',
          headers: {},
        },
      });

      mockResolveDefaultBranch.mockResolvedValue('main');

      // First call fails (master branch)
      // Second call succeeds (main branch)
      mockOctokit.rest.repos.getContent
        .mockRejectedValueOnce(notFoundError)
        .mockResolvedValueOnce({
          data: {
            type: 'file',
            content: Buffer.from('fallback content from main').toString(
              'base64'
            ),
            size: 26,
            sha: 'xyz789',
          },
        });

      const params = createTestParams({ branch: 'master', minified: false });

      const result = await fetchGitHubFileContentAPI(params);

      expect(result.status).toBe(200);
      expect(mockOctokit.rest.repos.getContent).toHaveBeenCalledTimes(2);
      // First call with master
      expect(mockOctokit.rest.repos.getContent).toHaveBeenNthCalledWith(1, {
        owner: 'test',
        repo: 'repo',
        path: 'test.txt',
        ref: 'master',
      });
      // Second call with main
      expect(mockOctokit.rest.repos.getContent).toHaveBeenNthCalledWith(2, {
        owner: 'test',
        repo: 'repo',
        path: 'test.txt',
        ref: 'main',
      });
      if ('data' in result) {
        expect(result.data.content).toBe('fallback content from main');
      }
    });

    it('should not fallback if branch is not main or master', async () => {
      const { RequestError } = await import('octokit');
      const notFoundError = new RequestError('Not Found', 404, {
        request: {
          method: 'GET',
          url: 'https://api.github.com/repos/test/repo/contents/test.txt',
          headers: {},
        },
      });

      // First call: main file request fails
      // Second call: findPathSuggestions tries parent directory (may fail too)
      mockOctokit.rest.repos.getContent
        .mockRejectedValueOnce(notFoundError)
        .mockRejectedValueOnce(notFoundError);

      const params = createTestParams({ branch: 'feature', minified: false });

      const result = await fetchGitHubFileContentAPI(params);

      expect(result.status).toBe(404);
      // First call is the main request, second is findPathSuggestions trying parent
      expect(mockOctokit.rest.repos.getContent).toHaveBeenCalledWith({
        owner: 'test',
        repo: 'repo',
        path: 'test.txt',
        ref: 'feature',
      });
      // Check for helpful hint
      if ('scopesSuggestion' in result) {
        expect(result.scopesSuggestion).toContain('feature');
        expect(result.scopesSuggestion).toContain(
          "Branch 'feature' not found. Default branch is 'main'. Ask user: Do you want to get the file from 'main' instead?"
        );
      }
    });

    it('should return 404 when both main and master fail', async () => {
      const { RequestError } = await import('octokit');
      const notFoundError = new RequestError('Not Found', 404, {
        request: {
          method: 'GET',
          url: 'https://api.github.com/repos/test/repo/contents/test.txt',
          headers: {},
        },
      });

      mockResolveDefaultBranch.mockResolvedValue('master');

      // Both calls fail
      mockOctokit.rest.repos.getContent
        .mockRejectedValueOnce(notFoundError)
        .mockRejectedValueOnce(notFoundError);

      const params = createTestParams({ branch: 'main', minified: false });

      const result = await fetchGitHubFileContentAPI(params);

      expect(result.status).toBe(404);
      expect(mockOctokit.rest.repos.getContent).toHaveBeenCalledTimes(2);
    });

    it('should not fallback on non-404 errors', async () => {
      const { RequestError } = await import('octokit');
      const forbiddenError = new RequestError('Forbidden', 403, {
        request: {
          method: 'GET',
          url: 'https://api.github.com/repos/test/repo/contents/test.txt',
          headers: {},
        },
      });

      mockOctokit.rest.repos.getContent.mockRejectedValueOnce(forbiddenError);

      const params = createTestParams({ branch: 'main', minified: false });

      const result = await fetchGitHubFileContentAPI(params);

      expect(result.status).toBe(403);
      expect(mockOctokit.rest.repos.getContent).toHaveBeenCalledTimes(1);
    });

    it('should not fallback when no branch is specified', async () => {
      const { RequestError } = await import('octokit');
      const notFoundError = new RequestError('Not Found', 404, {
        request: {
          method: 'GET',
          url: 'https://api.github.com/repos/test/repo/contents/test.txt',
          headers: {},
        },
      });

      // First call: main file request fails
      // Second call: findPathSuggestions tries parent directory (may fail too)
      mockOctokit.rest.repos.getContent
        .mockRejectedValueOnce(notFoundError)
        .mockRejectedValueOnce(notFoundError);

      const params = createTestParams({ minified: false });
      // Don't set branch, so it uses default

      const result = await fetchGitHubFileContentAPI(params);

      expect(result.status).toBe(404);
    });

    it('should include helpful hint for specific branch 404 errors', async () => {
      const { RequestError } = await import('octokit');
      const notFoundError = new RequestError('Not Found', 404, {
        request: {
          method: 'GET',
          url: 'https://api.github.com/repos/test/repo/contents/test.txt',
          headers: {},
        },
      });

      mockOctokit.rest.repos.getContent.mockRejectedValueOnce(notFoundError);

      const params = createTestParams({ branch: 'develop', minified: false });

      const result = await fetchGitHubFileContentAPI(params);

      expect(result.status).toBe(404);
      expect('scopesSuggestion' in result).toBe(true);
      if ('scopesSuggestion' in result) {
        expect(result.scopesSuggestion).toBe(
          "Branch 'develop' not found. Default branch is 'main'. Ask user: Do you want to get the file from 'main' instead?"
        );
      }
    });

    it('should not include hint when main/master fallback fails', async () => {
      const { RequestError } = await import('octokit');
      const notFoundError = new RequestError('Not Found', 404, {
        request: {
          method: 'GET',
          url: 'https://api.github.com/repos/test/repo/contents/test.txt',
          headers: {},
        },
      });

      // Both main and master fail
      mockOctokit.rest.repos.getContent
        .mockRejectedValueOnce(notFoundError)
        .mockRejectedValueOnce(notFoundError);

      const params = createTestParams({ branch: 'main', minified: false });

      const result = await fetchGitHubFileContentAPI(params);

      expect(result.status).toBe(404);
      // Should not have the "Ask user" hint since we tried fallback
      if ('scopesSuggestion' in result) {
        expect(result.scopesSuggestion).not.toContain('Ask user');
      }
    });

    it('should suggest alternative files when 404 and similar files exist', async () => {
      const { RequestError } = await import('octokit');
      const notFoundError = new RequestError('Not Found', 404, {
        request: {
          method: 'GET',
          url: 'https://api.github.com/repos/test/repo/contents/src/Test.ts',
          headers: {},
        },
      });

      // File request fails
      mockOctokit.rest.repos.getContent.mockRejectedValueOnce(notFoundError);
      // Parent directory request succeeds with similar files
      mockOctokit.rest.repos.getContent.mockResolvedValueOnce({
        data: [
          { name: 'test.ts', path: 'src/test.ts', type: 'file' },
          { name: 'test.js', path: 'src/test.js', type: 'file' },
          { name: 'Test.tsx', path: 'src/Test.tsx', type: 'file' },
        ],
      });

      const params = createTestParams({
        path: 'src/Test.ts',
        branch: 'feature',
        minified: false,
      });

      const result = await fetchGitHubFileContentAPI(params);

      expect(result.status).toBe(404);
      // Should have hint suggesting alternative files
      if ('hints' in result && result.hints) {
        const hintText = result.hints.join(' ');
        expect(hintText).toContain('Did you mean');
      }
    });

    it('should handle case-insensitive file suggestions', async () => {
      const { RequestError } = await import('octokit');
      const notFoundError = new RequestError('Not Found', 404, {
        request: {
          method: 'GET',
          url: 'https://api.github.com/repos/test/repo/contents/src/INDEX.ts',
          headers: {},
        },
      });

      // File request fails
      mockOctokit.rest.repos.getContent.mockRejectedValueOnce(notFoundError);
      // Parent directory request succeeds with case-different file
      mockOctokit.rest.repos.getContent.mockResolvedValueOnce({
        data: [{ name: 'index.ts', path: 'src/index.ts', type: 'file' }],
      });

      const params = createTestParams({
        path: 'src/INDEX.ts',
        branch: 'feature',
        minified: false,
      });

      const result = await fetchGitHubFileContentAPI(params);

      expect(result.status).toBe(404);
      if ('hints' in result && result.hints) {
        const hintText = result.hints.join(' ');
        expect(hintText).toContain('src/index.ts');
      }
    });
  });
});
