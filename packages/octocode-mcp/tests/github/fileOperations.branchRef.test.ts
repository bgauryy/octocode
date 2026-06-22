import { describe, it, expect, vi, beforeEach } from 'vitest';
import { fetchGitHubFileContentAPI } from '../../../octocode-tools-core/src/github/fileContent.js';
import {
  getOctokit,
  resolveDefaultBranch,
} from '../../../octocode-tools-core/src/github/client.js';
import { clearAllCache } from '../../../octocode-tools-core/src/utils/http/cache.js';
import { RequestError } from 'octokit';
import * as minifierModule from '@octocodeai/octocode-engine';

vi.mock('../../../octocode-tools-core/src/github/client.js');
vi.mock('@octocodeai/octocode-engine', async importOriginal => {
  const actual = await importOriginal();
  return { ...actual, minifyContent: vi.fn(), minifyContentSync: vi.fn() };
});

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
                sha: 'abc123def456',
                name: 'test.txt',
                path: 'test.txt',
              },
            }),
            listCommits: vi.fn().mockResolvedValue({ data: [] }),
          },
        },
      };

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as Awaited<ReturnType<typeof getOctokit>>
      );

      const result = await fetchGitHubFileContentAPI({
        owner: 'test',
        repo: 'repo',
        path: 'test.txt',
        branch: 'main',
      });

      expect(result).toHaveProperty('data');
      if ('data' in result && result.data) {
        expect(result.data.branch).toBe('main');
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
                sha: 'abc123def456789',
                name: 'test.txt',
                path: 'test.txt',
              },
            }),
            listCommits: vi.fn().mockResolvedValue({ data: [] }),
          },
        },
      };

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as Awaited<ReturnType<typeof getOctokit>>
      );

      const result = await fetchGitHubFileContentAPI({
        owner: 'test',
        repo: 'repo',
        path: 'no-branch.txt',
      });

      expect(result).toHaveProperty('data');
      if ('data' in result && result.data) {
        expect(result.data.branch).not.toBe('abc123def456789');
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
              .mockRejectedValueOnce(createRequestError('Not Found', 404))
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
        mockOctokit as unknown as Awaited<ReturnType<typeof getOctokit>>
      );

      const result = await fetchGitHubFileContentAPI({
        owner: 'test',
        repo: 'repo',
        path: 'fallback-test.txt',
        branch: 'main',
      });

      expect(result).toHaveProperty('data');
      if ('data' in result && result.data) {
        expect(result.data.branch).toBe('develop');
        expect(result.data.branch).not.toBe('blobsha123');
      }
    });
  });

  describe('pagination hints with branch field', () => {
    it('should NOT include branch SHA in pagination hints', async () => {
      const largeContent = 'x'.repeat(25000);

      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi.fn().mockResolvedValue({
              data: {
                type: 'file',
                content: Buffer.from(largeContent).toString('base64'),
                size: largeContent.length,
                sha: 'abc123def456789012345678901234567890abcd',
                name: 'large.txt',
                path: 'large.txt',
              },
            }),
            listCommits: vi.fn().mockResolvedValue({ data: [] }),
          },
        },
      };

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as Awaited<ReturnType<typeof getOctokit>>
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
        path: 'large-no-branch.txt',
      });

      expect(result).toHaveProperty('data');
      if ('data' in result && result.data && result.data.hints) {
        const allHints = result.data.hints.join(' ');
        expect(allHints).not.toContain(
          'abc123def456789012345678901234567890abcd'
        );
        if (allHints.includes('branch=')) {
          const branchMatch = allHints.match(/branch="([^"]+)"/);
          if (branchMatch && branchMatch[1]) {
            expect(branchMatch[1].length).toBeLessThan(40);
          }
        }
      }
    });

    it('should omit branch param from hints when branch is undefined', async () => {
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
        mockOctokit as unknown as Awaited<ReturnType<typeof getOctokit>>
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
      });

      expect(result).toHaveProperty('data');
      if ('data' in result && result.data && result.data.hints) {
        const sameParamsHint = result.data.hints.find(h =>
          h.includes('Same params')
        );
        if (sameParamsHint) {
          expect(sameParamsHint).toContain('owner="test"');
          expect(sameParamsHint).toContain('repo="repo"');
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
        mockOctokit as unknown as Awaited<ReturnType<typeof getOctokit>>
      );

      const result = await fetchGitHubFileContentAPI({
        owner: 'test',
        repo: 'repo',
        path: 'empty-branch.txt',
        branch: '',
      });

      expect(result).toHaveProperty('data');
      if ('data' in result && result.data) {
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
        mockOctokit as unknown as Awaited<ReturnType<typeof getOctokit>>
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
        path: 'lines-test.txt',
        branch: 'release/v2.0',
        startLine: 2,
        endLine: 4,
      });

      expect(result).toHaveProperty('data');
      if ('data' in result && result.data) {
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
        mockOctokit as unknown as Awaited<ReturnType<typeof getOctokit>>
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
        expect(result.data.branch).toBe('feature/search');
        expect(result.data.branch).not.toBe('matchSHA');
      }
    });
  });
});
