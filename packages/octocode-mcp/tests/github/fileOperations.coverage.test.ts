import { describe, it, expect, vi, beforeEach } from 'vitest';
import { fetchGitHubFileContentAPI } from '../../src/github/fileContent.js';
import { viewGitHubRepositoryStructureAPI } from '../../src/github/repoStructure.js';
import { getOctokit, resolveDefaultBranch } from '../../src/github/client.js';
import { clearAllCache } from '../../src/utils/http/cache.js';
import { RequestError } from 'octokit';
import * as minifierModule from '../../src/utils/minifier/minifier.js';

vi.mock('../../src/github/client.js');
vi.mock('../../src/session.js', () => ({
  logSessionError: vi.fn(() => Promise.resolve()),
}));
vi.mock('../../src/utils/minifier/minifier.js');

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

describe('File Operations - Additional Coverage Tests', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    clearAllCache();
    vi.mocked(resolveDefaultBranch).mockResolvedValue('main');
  });

  describe('fetchFileTimestamp coverage', () => {
    it('should extract lastModified from commit data with author.name', async () => {
      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi.fn().mockResolvedValue({
              data: {
                type: 'file',
                content: Buffer.from('test content').toString('base64'),
                size: 12,
                sha: 'abc123',
                name: 'test.txt',
                path: 'test.txt',
              },
            }),
            listCommits: vi.fn().mockResolvedValue({
              data: [
                {
                  commit: {
                    author: {
                      name: 'John Doe',
                    },
                    committer: {
                      date: '2024-01-15T10:30:00Z',
                    },
                  },
                  author: {
                    login: 'johndoe',
                  },
                },
              ],
            }),
          },
        },
      };

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );
      vi.mocked(minifierModule.minifyContent).mockResolvedValue({
        content: 'test content',
        failed: false,
        type: 'general',
      });

      const result = await fetchGitHubFileContentAPI({
        owner: 'test',
        repo: 'repo',
        path: 'test.txt',
      });

      expect(result).toHaveProperty('data');
      if ('data' in result && result.data) {
        expect(result.data.lastModified).toBe('2024-01-15T10:30:00Z');
        expect(result.data.lastModifiedBy).toBe('John Doe');
      }
    });

    it('should fallback to author.login when commit.author.name is missing', async () => {
      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi.fn().mockResolvedValue({
              data: {
                type: 'file',
                content: Buffer.from('test content').toString('base64'),
                size: 12,
                sha: 'abc123',
                name: 'test.txt',
                path: 'test.txt',
              },
            }),
            listCommits: vi.fn().mockResolvedValue({
              data: [
                {
                  commit: {
                    author: {},
                    committer: {
                      date: '2024-01-15T10:30:00Z',
                    },
                  },
                  author: {
                    login: 'johndoe',
                  },
                },
              ],
            }),
          },
        },
      };

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );
      vi.mocked(minifierModule.minifyContent).mockResolvedValue({
        content: 'test content',
        failed: false,
        type: 'general',
      });

      const result = await fetchGitHubFileContentAPI({
        owner: 'test',
        repo: 'repo',
        path: 'test2.txt',
      });

      expect(result).toHaveProperty('data');
      if ('data' in result && result.data) {
        expect(result.data.lastModifiedBy).toBe('johndoe');
      }
    });

    it('should use Unknown when no author info available', async () => {
      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi.fn().mockResolvedValue({
              data: {
                type: 'file',
                content: Buffer.from('test content').toString('base64'),
                size: 12,
                sha: 'abc123',
                name: 'test.txt',
                path: 'test.txt',
              },
            }),
            listCommits: vi.fn().mockResolvedValue({
              data: [
                {
                  commit: {
                    author: {},
                    committer: {
                      date: '2024-01-15T10:30:00Z',
                    },
                  },
                  author: null,
                },
              ],
            }),
          },
        },
      };

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );
      vi.mocked(minifierModule.minifyContent).mockResolvedValue({
        content: 'test content',
        failed: false,
        type: 'general',
      });

      const result = await fetchGitHubFileContentAPI({
        owner: 'test',
        repo: 'repo',
        path: 'test3.txt',
      });

      expect(result).toHaveProperty('data');
      if ('data' in result && result.data) {
        expect(result.data.lastModifiedBy).toBe('Unknown');
      }
    });

    it('should use Unknown for date when committer.date is missing', async () => {
      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi.fn().mockResolvedValue({
              data: {
                type: 'file',
                content: Buffer.from('test content').toString('base64'),
                size: 12,
                sha: 'abc123',
                name: 'test.txt',
                path: 'test.txt',
              },
            }),
            listCommits: vi.fn().mockResolvedValue({
              data: [
                {
                  commit: {
                    author: { name: 'John' },
                    committer: {},
                  },
                  author: null,
                },
              ],
            }),
          },
        },
      };

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );
      vi.mocked(minifierModule.minifyContent).mockResolvedValue({
        content: 'test content',
        failed: false,
        type: 'general',
      });

      const result = await fetchGitHubFileContentAPI({
        owner: 'test',
        repo: 'repo',
        path: 'test4.txt',
      });

      expect(result).toHaveProperty('data');
      if ('data' in result && result.data) {
        expect(result.data.lastModified).toBe('Unknown');
      }
    });
  });

  describe('findPathSuggestions coverage', () => {
    it('should provide case-insensitive path suggestions on 404', async () => {
      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi
              .fn()
              .mockRejectedValueOnce(createRequestError('Not Found', 404))
              .mockResolvedValueOnce({
                data: [
                  {
                    name: 'README.MD',
                    path: 'README.MD',
                    type: 'file',
                  },
                  {
                    name: 'readme.txt',
                    path: 'readme.txt',
                    type: 'file',
                  },
                ],
              }),
            get: vi.fn().mockResolvedValue({
              data: { default_branch: 'main' },
            }),
          },
        },
      };

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );

      const result = await fetchGitHubFileContentAPI({
        owner: 'test',
        repo: 'repo',
        path: 'readme.md',
        branch: 'main',
      });

      expect('error' in result).toBe(true);
      if ('error' in result) {
        expect(result.hints).toBeDefined();
        expect(result.hints?.some(h => h.includes('Did you mean'))).toBe(true);
        expect(result.hints?.some(h => h.includes('README.MD'))).toBe(true);
      }
    });

    it('should suggest extension alternatives on 404', async () => {
      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi
              .fn()
              .mockRejectedValueOnce(createRequestError('Not Found', 404))
              .mockResolvedValueOnce({
                data: [
                  {
                    name: 'config.js',
                    path: 'src/config.js',
                    type: 'file',
                  },
                  {
                    name: 'config.json',
                    path: 'src/config.json',
                    type: 'file',
                  },
                ],
              }),
            get: vi.fn().mockResolvedValue({
              data: { default_branch: 'main' },
            }),
          },
        },
      };

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );

      const result = await fetchGitHubFileContentAPI({
        owner: 'test',
        repo: 'repo',
        path: 'src/config.ts',
        branch: 'main',
      });

      expect('error' in result).toBe(true);
      if ('error' in result) {
        expect(result.hints).toBeDefined();
        expect(result.hints?.some(h => h.includes('Did you mean'))).toBe(true);
      }
    });

    it('should suggest prefix-match file when name is a typo extension of real file', async () => {
      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi
              .fn()
              .mockRejectedValueOnce(createRequestError('Not Found', 404))
              .mockResolvedValueOnce({
                data: [
                  {
                    name: 'npm.ts',
                    path: 'src/utils/package/npm.ts',
                    type: 'file',
                  },
                  {
                    name: 'types.ts',
                    path: 'src/utils/package/types.ts',
                    type: 'file',
                  },
                ],
              }),
            get: vi.fn().mockResolvedValue({
              data: { default_branch: 'main' },
            }),
          },
        },
      };

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );

      const result = await fetchGitHubFileContentAPI({
        owner: 'test',
        repo: 'repo',
        path: 'src/utils/package/npm_typo.ts',
      });

      expect('error' in result).toBe(true);
      if ('error' in result) {
        expect(result.hints).toBeDefined();
        expect(result.hints?.some(h => h.includes('Did you mean'))).toBe(true);
        expect(result.hints?.some(h => h.includes('npm.ts'))).toBe(true);
      }
    });

    it('should not suggest prefix matches when base name is too short (< 3 chars)', async () => {
      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi
              .fn()
              .mockRejectedValueOnce(createRequestError('Not Found', 404))
              .mockResolvedValueOnce({
                data: [{ name: 'io.ts', path: 'src/io.ts', type: 'file' }],
              }),
            get: vi.fn().mockResolvedValue({
              data: { default_branch: 'main' },
            }),
          },
        },
      };

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );

      const result = await fetchGitHubFileContentAPI({
        owner: 'test',
        repo: 'repo',
        path: 'src/io_x.ts',
      });

      expect('error' in result).toBe(true);
      if ('error' in result) {
        // "io" is < 3 chars so prefix match should NOT fire
        expect(result.hints?.some(h => h.includes('Did you mean'))).toBeFalsy();
      }
    });

    it('should handle findPathSuggestions error gracefully', async () => {
      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi
              .fn()
              .mockRejectedValueOnce(createRequestError('Not Found', 404))
              .mockRejectedValueOnce(new Error('Network error')),
            get: vi.fn().mockResolvedValue({
              data: { default_branch: 'main' },
            }),
          },
        },
      };

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );

      const result = await fetchGitHubFileContentAPI({
        owner: 'test',
        repo: 'repo',
        path: 'nonexistent.txt',
        branch: 'main',
      });

      expect('error' in result).toBe(true);
      if ('error' in result) {
        expect(result.status).toBe(404);
      }
    });

    it('should return empty suggestions when parent is not a directory', async () => {
      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi
              .fn()
              .mockRejectedValueOnce(createRequestError('Not Found', 404))
              .mockResolvedValueOnce({
                data: {
                  name: 'parent',
                  path: 'parent',
                  type: 'file',
                },
              }),
            get: vi.fn().mockResolvedValue({
              data: { default_branch: 'main' },
            }),
          },
        },
      };

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );

      const result = await fetchGitHubFileContentAPI({
        owner: 'test',
        repo: 'repo',
        path: 'parent/child.txt',
        branch: 'main',
      });

      expect('error' in result).toBe(true);
      if ('error' in result) {
        expect(result.status).toBe(404);
        expect(result.hints?.some(h => h.includes('Did you mean'))).toBeFalsy();
      }
    });
  });

  describe('Branch fallback with path suggestions', () => {
    it('should suggest default branch when requested branch not found', async () => {
      vi.mocked(resolveDefaultBranch).mockResolvedValue('develop');

      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi
              .fn()
              .mockRejectedValueOnce(createRequestError('Not Found', 404))
              .mockResolvedValueOnce({
                data: [],
              }),
            get: vi.fn().mockResolvedValue({
              data: { default_branch: 'develop' },
            }),
          },
        },
      };

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );

      const result = await fetchGitHubFileContentAPI({
        owner: 'test',
        repo: 'repo',
        path: 'file.txt',
        branch: 'feature-branch',
      });

      expect('error' in result).toBe(true);
      if ('error' in result) {
        expect(result.scopesSuggestion).toContain('develop');
        expect(result.scopesSuggestion).toContain(
          'Do you want to get the file from'
        );
      }
    });
  });

  describe('viewGitHubRepositoryStructureAPI error paths', () => {
    it('should return error when path not found on any branch', async () => {
      vi.mocked(resolveDefaultBranch).mockResolvedValue('custom-default');

      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi
              .fn()
              .mockRejectedValueOnce(createRequestError('Not Found', 404)),
          },
        },
      };

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );

      const result = await viewGitHubRepositoryStructureAPI({
        owner: 'test',
        repo: 'repo',
        branch: 'nonexistent',
        path: 'nonexistent/path',
      });

      expect('error' in result).toBe(true);
      if ('error' in result) {
        expect(result.error).toContain('not found');
        expect(result.triedBranches).toBeUndefined();
        expect(result.defaultBranch).toBeUndefined();
      }
    });

    it('should return error when path not found and branch is the default', async () => {
      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi
              .fn()
              .mockRejectedValueOnce(createRequestError('Not Found', 404)),
            get: vi.fn().mockResolvedValue({
              data: { default_branch: 'main' },
            }),
          },
        },
      };

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );

      const result = await viewGitHubRepositoryStructureAPI({
        owner: 'test',
        repo: 'repo',
        branch: 'main',
        path: 'nonexistent/path',
      });

      expect('error' in result).toBe(true);
      if ('error' in result) {
        expect(result.error).toContain('not found');
        expect(result.status).toBe(404);
      }
    });

    it('should handle non-404 errors', async () => {
      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi
              .fn()
              .mockRejectedValueOnce(createRequestError('Forbidden', 403)),
            get: vi.fn().mockResolvedValue({
              data: { default_branch: 'main' },
            }),
          },
        },
      };

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );

      const result = await viewGitHubRepositoryStructureAPI({
        owner: 'test',
        repo: 'repo',
        branch: 'main',
        path: '',
      });

      expect('error' in result).toBe(true);
      if ('error' in result) {
        expect(result.status).toBe(403);
      }
    });

    it('should return error when repo not accessible', async () => {
      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi
              .fn()
              .mockRejectedValueOnce(createRequestError('Not Found', 404)),
            get: vi
              .fn()
              .mockRejectedValueOnce(createRequestError('Not Found', 404)),
          },
        },
      };

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );

      const result = await viewGitHubRepositoryStructureAPI({
        owner: 'test',
        repo: 'nonexistent',
        branch: 'main',
        path: '',
      });

      expect('error' in result).toBe(true);
      if ('error' in result) {
        expect(result.error).toContain('not found');
        expect(result.status).toBe(404);
      }
    });

    it('should handle generic errors in structure exploration', async () => {
      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi.fn().mockRejectedValue(new Error('Network error')),
            get: vi.fn().mockResolvedValue({
              data: { default_branch: 'main' },
            }),
          },
        },
      };

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );

      const result = await viewGitHubRepositoryStructureAPI({
        owner: 'test',
        repo: 'repo',
        branch: 'main',
        path: '',
      });

      expect('error' in result).toBe(true);
      if ('error' in result) {
        expect(result.error).toContain('Failed to access');
      }
    });
  });

  describe('viewGitHubRepositoryStructureAPI path normalization', () => {
    it('should strip trailing slashes from path before calling GitHub API', async () => {
      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi.fn().mockResolvedValue({
              data: [
                {
                  name: 'core',
                  path: 'packages/core',
                  type: 'dir',
                  url: 'url',
                  html_url: 'html',
                  git_url: 'git',
                  sha: 'sha1',
                },
              ],
            }),
            get: vi.fn().mockResolvedValue({
              data: { default_branch: 'main' },
            }),
          },
        },
      };

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );

      const result = await viewGitHubRepositoryStructureAPI({
        owner: 'facebook',
        repo: 'react',
        branch: 'main',
        path: 'packages/',
      });

      expect('structure' in result).toBe(true);
      expect(mockOctokit.rest.repos.getContent).toHaveBeenCalledWith(
        expect.objectContaining({
          path: 'packages',
        })
      );
    });

    it('should strip both leading and trailing slashes from path', async () => {
      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi.fn().mockResolvedValue({
              data: [
                {
                  name: 'index.ts',
                  path: 'src/index.ts',
                  type: 'file',
                  size: 100,
                  url: 'url',
                  html_url: 'html',
                  git_url: 'git',
                  sha: 'sha1',
                },
              ],
            }),
            get: vi.fn().mockResolvedValue({
              data: { default_branch: 'main' },
            }),
          },
        },
      };

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );

      const result = await viewGitHubRepositoryStructureAPI({
        owner: 'test',
        repo: 'repo',
        branch: 'main',
        path: '/src/',
      });

      expect('structure' in result).toBe(true);
      expect(mockOctokit.rest.repos.getContent).toHaveBeenCalledWith(
        expect.objectContaining({
          path: 'src',
        })
      );
    });

    it('should handle path with multiple trailing slashes', async () => {
      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi.fn().mockResolvedValue({
              data: [
                {
                  name: 'file.ts',
                  path: 'lib/file.ts',
                  type: 'file',
                  size: 50,
                  url: 'url',
                  html_url: 'html',
                  git_url: 'git',
                  sha: 'sha1',
                },
              ],
            }),
            get: vi.fn().mockResolvedValue({
              data: { default_branch: 'main' },
            }),
          },
        },
      };

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );

      const result = await viewGitHubRepositoryStructureAPI({
        owner: 'test',
        repo: 'repo',
        branch: 'main',
        path: 'lib///',
      });

      expect('structure' in result).toBe(true);
      expect(mockOctokit.rest.repos.getContent).toHaveBeenCalledWith(
        expect.objectContaining({
          path: 'lib',
        })
      );
    });
  });

  describe('Recursive fetch error handling', () => {
    it('should handle errors in recursive directory fetching gracefully', async () => {
      const mockOctokit = {
        rest: {
          repos: {
            get: vi.fn().mockResolvedValue({
              data: { default_branch: 'main' },
            }),
            getContent: vi
              .fn()
              .mockResolvedValueOnce({
                data: [
                  {
                    name: 'dir1',
                    path: 'dir1',
                    type: 'dir',
                    url: 'url',
                    html_url: 'html',
                    git_url: 'git',
                    sha: 'sha1',
                  },
                  {
                    name: 'dir2',
                    path: 'dir2',
                    type: 'dir',
                    url: 'url',
                    html_url: 'html',
                    git_url: 'git',
                    sha: 'sha2',
                  },
                ],
              })
              .mockRejectedValueOnce(new Error('Access denied'))
              .mockResolvedValueOnce({
                data: [
                  {
                    name: 'file.ts',
                    path: 'dir2/file.ts',
                    type: 'file',
                    size: 100,
                    url: 'url',
                    html_url: 'html',
                    git_url: 'git',
                    sha: 'sha3',
                  },
                ],
              }),
          },
        },
      };

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );

      const result = await viewGitHubRepositoryStructureAPI({
        owner: 'test',
        repo: 'repo',
        branch: 'main',
        path: '',
        depth: 2,
      });

      expect('structure' in result).toBe(true);
      if ('structure' in result) {
        expect(result.structure['.']).toBeDefined();
        expect(result.structure['.']!.folders).toContain('dir1');
        expect(result.structure['.']!.folders).toContain('dir2');
        expect(result.summary.totalFolders).toBeGreaterThanOrEqual(2);
      }
    });
  });

  describe('File content with no content returned', () => {
    it('should handle file with null content field', async () => {
      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi.fn().mockResolvedValue({
              data: {
                type: 'file',
                content: null,
                size: 100,
                sha: 'abc123',
                name: 'test.txt',
                path: 'test.txt',
              },
            }),
          },
        },
      };

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );

      const result = await fetchGitHubFileContentAPI({
        owner: 'test',
        repo: 'repo',
        path: 'test-null.txt',
      });

      expect('error' in result).toBe(true);
      if ('error' in result) {
        expect(result.error).toContain('File is empty');
      }
    });
  });

  describe('Multiple match locations in matchString', () => {
    it('should report multiple match locations', async () => {
      const fileContent =
        'TODO: first\nLine 2\nTODO: second\nLine 4\nTODO: third';

      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi.fn().mockResolvedValue({
              data: {
                type: 'file',
                content: Buffer.from(fileContent).toString('base64'),
                size: fileContent.length,
                sha: 'abc123',
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
        path: 'multi-match.txt',
        matchString: 'TODO',
        matchStringContextLines: 1,
      });

      expect(result).toHaveProperty('data');
      if ('data' in result && result.data) {
        expect(result.data.matchLocations).toBeDefined();
        expect(
          result.data.matchLocations?.some(
            w => w.includes('2 other locations') || w.includes('other location')
          )
        ).toBe(true);
      }
    });
  });
});
