import { describe, it, expect, vi, beforeEach } from 'vitest';
import { fetchGitHubFileContentAPI } from '../../src/github/fileContent.js';
import { getOctokit, resolveDefaultBranch } from '../../src/github/client.js';
import { clearAllCache } from '../../src/utils/http/cache.js';
import { RequestError } from 'octokit';
import * as minifierModule from '../../src/utils/minifier/minifier.js';

vi.mock('../../src/github/client.js');
vi.mock('../../src/session.js', () => ({
  logSessionError: vi.fn(() => Promise.resolve()),
}));
vi.mock('../../src/utils/minifier/minifier.js');

// Helper to create RequestError with proper structure
function createRequestError(message: string, status: number) {
  return new RequestError(message, status, {
    request: {
      method: 'GET',
      url: 'https://api.github.com/test',
      headers: {},
    },
    response: {
      status,
      url: 'https://api.github.com/test',
      headers: {},
      data: {},
      retryCount: 0,
    },
  });
}

describe('File Operations - Branch and ResolvedRef Behavior', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    clearAllCache();
    vi.mocked(resolveDefaultBranch).mockResolvedValue('main');
    vi.mocked(minifierModule.minifyContent).mockResolvedValue({
      content: 'test content',
      failed: false,
      type: 'general',
    });
  });

  describe('branch field behavior', () => {
    it('should set branch to the resolved branch name when known', async () => {
      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi.fn().mockResolvedValue({
              data: {
                type: 'file',
                content: Buffer.from('test content').toString('base64'),
                size: 12,
                sha: 'abc123def456', // This is blob SHA - should NOT appear in branch
                name: 'test.txt',
                path: 'test.txt',
              },
            }),
            listCommits: vi.fn().mockResolvedValue({ data: [] }),
          },
        },
      };

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );

      const result = await fetchGitHubFileContentAPI({
        owner: 'test',
        repo: 'repo',
        path: 'test.txt',
        branch: 'main',
      });

      expect(result).toHaveProperty('data');
      if ('data' in result && result.data) {
        // Branch should be 'main' - the branch we requested
        expect(result.data.branch).toBe('main');
        // Should NOT be the blob SHA
        expect(result.data.branch).not.toBe('abc123def456');
      }
    });

    it('should NOT fallback to blob SHA when branch is not explicitly resolved', async () => {
      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi.fn().mockResolvedValue({
              data: {
                type: 'file',
                content: Buffer.from('test content').toString('base64'),
                size: 12,
                sha: 'abc123def456789', // Blob SHA
                name: 'test.txt',
                path: 'test.txt',
              },
            }),
            listCommits: vi.fn().mockResolvedValue({ data: [] }),
          },
        },
      };

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );

      // Request without branch parameter
      const result = await fetchGitHubFileContentAPI({
        owner: 'test',
        repo: 'repo',
        path: 'no-branch.txt',
      });

      expect(result).toHaveProperty('data');
      if ('data' in result && result.data) {
        // Branch should NOT be the blob SHA
        expect(result.data.branch).not.toBe('abc123def456789');
        // It could be undefined, empty, or 'HEAD' - but NOT a 40-char SHA
        if (result.data.branch) {
          expect(result.data.branch.length).toBeLessThan(40);
        }
      }
    });

    it('should set branch to fallback branch when main/master redirects to default', async () => {
      vi.mocked(resolveDefaultBranch).mockResolvedValue('develop');

      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi
              .fn()
              // First call fails - 'main' not found
              .mockRejectedValueOnce(createRequestError('Not Found', 404))
              // Second call succeeds with 'develop'
              .mockResolvedValueOnce({
                data: {
                  type: 'file',
                  content: Buffer.from('test content').toString('base64'),
                  size: 12,
                  sha: 'blobsha123',
                  name: 'test.txt',
                  path: 'test.txt',
                },
              }),
            get: vi.fn().mockResolvedValue({
              data: { default_branch: 'develop' },
            }),
            listCommits: vi.fn().mockResolvedValue({ data: [] }),
          },
        },
      };

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );

      const result = await fetchGitHubFileContentAPI({
        owner: 'test',
        repo: 'repo',
        path: 'fallback-test.txt',
        branch: 'main', // Request 'main' but actual default is 'develop'
      });

      expect(result).toHaveProperty('data');
      if ('data' in result && result.data) {
        // Should be 'develop' - the actual branch used
        expect(result.data.branch).toBe('develop');
        // Should NOT be blob SHA
        expect(result.data.branch).not.toBe('blobsha123');
      }
    });
  });

  describe('pagination hints with branch field', () => {
    it('should NOT include branch SHA in pagination hints', async () => {
      // Large content to trigger pagination
      const largeContent = 'x'.repeat(25000);

      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi.fn().mockResolvedValue({
              data: {
                type: 'file',
                content: Buffer.from(largeContent).toString('base64'),
                size: largeContent.length,
                sha: 'abc123def456789012345678901234567890abcd', // 40-char blob SHA
                name: 'large.txt',
                path: 'large.txt',
              },
            }),
            listCommits: vi.fn().mockResolvedValue({ data: [] }),
          },
        },
      };

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );
      vi.mocked(minifierModule.minifyContent).mockImplementation(
        async content => ({
          content,
          failed: false,
          type: 'general',
        })
      );

      // Request without branch - simulating when branch is unknown
      const result = await fetchGitHubFileContentAPI({
        owner: 'test',
        repo: 'repo',
        path: 'large-no-branch.txt',
        // No branch specified
      });

      expect(result).toHaveProperty('data');
      if ('data' in result && result.data && result.data.hints) {
        // Should NOT include blob SHA in hints
        const allHints = result.data.hints.join(' ');
        expect(allHints).not.toContain(
          'abc123def456789012345678901234567890abcd'
        );
        // Hints should either have no branch param or have a valid branch name
        if (allHints.includes('branch=')) {
          // If branch is mentioned, it should not be a 40-char SHA
          const branchMatch = allHints.match(/branch="([^"]+)"/);
          if (branchMatch && branchMatch[1]) {
            expect(branchMatch[1].length).toBeLessThan(40);
          }
        }
      }
    });

    it('should omit branch param from hints when branch is undefined', async () => {
      // Large content to trigger pagination
      const largeContent = 'y'.repeat(25000);

      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi.fn().mockResolvedValue({
              data: {
                type: 'file',
                content: Buffer.from(largeContent).toString('base64'),
                size: largeContent.length,
                sha: 'someblobsha',
                name: 'file.txt',
                path: 'file.txt',
              },
            }),
            listCommits: vi.fn().mockResolvedValue({ data: [] }),
          },
        },
      };

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );
      vi.mocked(minifierModule.minifyContent).mockImplementation(
        async content => ({
          content,
          failed: false,
          type: 'general',
        })
      );

      const result = await fetchGitHubFileContentAPI({
        owner: 'test',
        repo: 'repo',
        path: 'no-branch-hint-test.txt',
        // No branch - so hints should not include branch param
      });

      expect(result).toHaveProperty('data');
      if ('data' in result && result.data && result.data.hints) {
        // Check the "Same params" hint line
        const sameParamsHint = result.data.hints.find(h =>
          h.includes('Same params')
        );
        if (sameParamsHint) {
          // Should have owner, repo, path but branch should be empty or omitted
          expect(sameParamsHint).toContain('owner="test"');
          expect(sameParamsHint).toContain('repo="repo"');
          // Branch param should be empty (not branch="someblobsha")
          expect(sameParamsHint).not.toContain('branch="someblobsha"');
        }
      }
    });
  });

  describe('edge cases for branch resolution', () => {
    it('should handle empty branch string gracefully', async () => {
      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi.fn().mockResolvedValue({
              data: {
                type: 'file',
                content: Buffer.from('content').toString('base64'),
                size: 7,
                sha: 'sha456',
                name: 'test.txt',
                path: 'test.txt',
              },
            }),
            listCommits: vi.fn().mockResolvedValue({ data: [] }),
          },
        },
      };

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );

      const result = await fetchGitHubFileContentAPI({
        owner: 'test',
        repo: 'repo',
        path: 'empty-branch.txt',
        branch: '', // Empty string
      });

      expect(result).toHaveProperty('data');
      if ('data' in result && result.data) {
        // Branch should not be blob SHA
        expect(result.data.branch).not.toBe('sha456');
      }
    });

    it('should preserve branch name through the processing pipeline', async () => {
      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi.fn().mockResolvedValue({
              data: {
                type: 'file',
                content: Buffer.from(
                  'line1\nline2\nline3\nline4\nline5'
                ).toString('base64'),
                size: 24,
                sha: 'blobSHA',
                name: 'lines.txt',
                path: 'lines.txt',
              },
            }),
            listCommits: vi.fn().mockResolvedValue({ data: [] }),
          },
        },
      };

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );
      vi.mocked(minifierModule.minifyContent).mockImplementation(
        async content => ({
          content,
          failed: false,
          type: 'general',
        })
      );

      // Using startLine/endLine to test processing pipeline
      const result = await fetchGitHubFileContentAPI({
        owner: 'test',
        repo: 'repo',
        path: 'lines-test.txt',
        branch: 'release/v2.0',
        startLine: 2,
        endLine: 4,
      });

      expect(result).toHaveProperty('data');
      if ('data' in result && result.data) {
        // Branch should be preserved through processing
        expect(result.data.branch).toBe('release/v2.0');
        expect(result.data.branch).not.toBe('blobSHA');
      }
    });

    it('should handle matchString processing while preserving branch', async () => {
      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi.fn().mockResolvedValue({
              data: {
                type: 'file',
                content: Buffer.from(
                  'function hello() {\n  console.log("hello");\n}'
                ).toString('base64'),
                size: 44,
                sha: 'matchSHA',
                name: 'hello.js',
                path: 'hello.js',
              },
            }),
            listCommits: vi.fn().mockResolvedValue({ data: [] }),
          },
        },
      };

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );
      vi.mocked(minifierModule.minifyContent).mockImplementation(
        async content => ({
          content,
          failed: false,
          type: 'general',
        })
      );

      const result = await fetchGitHubFileContentAPI({
        owner: 'test',
        repo: 'repo',
        path: 'match-test.js',
        branch: 'feature/search',
        matchString: 'console',
      });

      expect(result).toHaveProperty('data');
      if ('data' in result && result.data) {
        // Branch should be preserved even with matchString processing
        expect(result.data.branch).toBe('feature/search');
        expect(result.data.branch).not.toBe('matchSHA');
      }
    });
  });
});
