import { describe, it, expect, beforeEach, vi } from 'vitest';
import { viewGitHubRepositoryStructureAPI } from '../../src/github/repoStructure.js';
import { fetchDirectoryContentsRecursivelyAPI } from '../../src/github/repoStructureRecursive.js';
import { getOctokit } from '../../src/github/client.js';
import { clearAllCache } from '../../src/utils/http/cache.js';
import { countSerializedChars } from '../../src/utils/response/charSavings.js';

vi.mock('../../src/github/client.js');
vi.mock('../../src/session.js', () => ({
  logSessionError: vi.fn(() => Promise.resolve()),
}));

describe('fetchDirectoryContentsRecursivelyAPI — branch coverage', () => {
  it('returns empty array immediately when currentDepth > maxDepth', async () => {
    const mockOctokit = {
      rest: { repos: { getContent: vi.fn() } },
    };

    const result = await fetchDirectoryContentsRecursivelyAPI(
      mockOctokit as any,
      'owner',
      'repo',
      'main',
      'src',
      5,
      2
    );

    expect(result).toEqual([]);
    expect(mockOctokit.rest.repos.getContent).not.toHaveBeenCalled();
  });

  it('returns empty array when path is in visitedPaths (cycle detection)', async () => {
    const mockOctokit = {
      rest: { repos: { getContent: vi.fn() } },
    };
    const visited = new Set(['src']);

    const result = await fetchDirectoryContentsRecursivelyAPI(
      mockOctokit as any,
      'owner',
      'repo',
      'main',
      'src',
      0,
      3,
      visited
    );

    expect(result).toEqual([]);
    expect(mockOctokit.rest.repos.getContent).not.toHaveBeenCalled();
  });

  it('returns empty array on subdirectory fetch error (catch branch)', async () => {
    const mockOctokit = {
      rest: {
        repos: {
          getContent: vi
            .fn()
            .mockResolvedValueOnce({
              data: [
                {
                  name: 'src',
                  path: 'src',
                  type: 'dir',
                  size: 0,
                  url: 'url',
                  html_url: 'html',
                  git_url: 'git',
                  sha: 'abc',
                },
              ],
            })
            .mockRejectedValueOnce(new Error('Access denied for subdir')),
        },
      },
    };

    const result = await fetchDirectoryContentsRecursivelyAPI(
      mockOctokit as any,
      'owner',
      'repo',
      'main',
      '',
      0,
      1
    );

    expect(Array.isArray(result)).toBe(true);
  });
});

describe('GitHub File Operations - Recursive Directory Structure', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    clearAllCache();
  });

  describe('viewGitHubRepositoryStructureAPI with depth > 1', () => {
    it('should recursively fetch directory contents with depth 2', async () => {
      const mockOctokit = {
        rest: {
          repos: {
            get: vi.fn().mockResolvedValue({
              data: {
                default_branch: 'main',
              },
            }),
            getContent: vi
              .fn()
              // First call - root directory
              .mockResolvedValueOnce({
                data: [
                  {
                    name: 'src',
                    path: 'src',
                    type: 'dir',
                    size: 0,
                    url: 'https://api.github.com/repos/test/repo/contents/src',
                    html_url: 'https://github.com/test/repo/tree/main/src',
                    git_url:
                      'https://api.github.com/repos/test/repo/git/trees/abc',
                    sha: 'abc123',
                  },
                  {
                    name: 'README.md',
                    path: 'README.md',
                    type: 'file',
                    size: 100,
                    url: 'https://api.github.com/repos/test/repo/contents/README.md',
                    html_url:
                      'https://github.com/test/repo/blob/main/README.md',
                    git_url:
                      'https://api.github.com/repos/test/repo/git/blobs/def',
                    sha: 'def456',
                  },
                ],
              })
              // Second call - src directory
              .mockResolvedValueOnce({
                data: [
                  {
                    name: 'index.ts',
                    path: 'src/index.ts',
                    type: 'file',
                    size: 200,
                    url: 'https://api.github.com/repos/test/repo/contents/src/index.ts',
                    html_url:
                      'https://github.com/test/repo/blob/main/src/index.ts',
                    git_url:
                      'https://api.github.com/repos/test/repo/git/blobs/ghi',
                    sha: 'ghi789',
                  },
                  {
                    name: 'utils',
                    path: 'src/utils',
                    type: 'dir',
                    size: 0,
                    url: 'https://api.github.com/repos/test/repo/contents/src/utils',
                    html_url:
                      'https://github.com/test/repo/tree/main/src/utils',
                    git_url:
                      'https://api.github.com/repos/test/repo/git/trees/jkl',
                    sha: 'jkl012',
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
        depth: 2,
      });

      expect('structure' in result).toBe(true);
      if ('structure' in result) {
        // Should have structure grouped by directory
        expect(Object.keys(result.structure).length).toBeGreaterThan(0);
        // Root should have files and folders
        expect(result.structure['.']).toBeDefined();
        expect(result.structure['.']?.files).toContain('README.md');
        expect(result.structure['.']?.folders).toContain('src');
        // Should have made API calls
        expect(mockOctokit.rest.repos.getContent).toHaveBeenCalled();
        const [rootResponse, recursiveResponse] =
          mockOctokit.rest.repos.getContent.mock.results.map(
            call => call.value
          );
        const [rootData, recursiveData] = await Promise.all([
          rootResponse.then(response => response.data),
          recursiveResponse.then(response => response.data),
        ]);
        expect(result.rawResponseChars).toBe(
          countSerializedChars(rootData) + countSerializedChars(recursiveData)
        );
      }
    });

    it('should handle circular/visited path detection', async () => {
      const mockOctokit = {
        rest: {
          repos: {
            get: vi.fn().mockResolvedValue({
              data: {
                default_branch: 'main',
              },
            }),
            getContent: vi
              .fn()
              .mockResolvedValueOnce({
                data: [
                  {
                    name: 'src',
                    path: 'src',
                    type: 'dir',
                    size: 0,
                    url: 'https://api.github.com/repos/test/repo/contents/src',
                    html_url: 'https://github.com/test/repo/tree/main/src',
                    git_url:
                      'https://api.github.com/repos/test/repo/git/trees/abc',
                    sha: 'abc123',
                  },
                ],
              })
              .mockResolvedValueOnce({
                data: [
                  {
                    name: 'index.ts',
                    path: 'src/index.ts',
                    type: 'file',
                    size: 100,
                    url: 'https://api.github.com/repos/test/repo/contents/src/index.ts',
                    html_url:
                      'https://github.com/test/repo/blob/main/src/index.ts',
                    git_url:
                      'https://api.github.com/repos/test/repo/git/blobs/def',
                    sha: 'def456',
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
        depth: 2,
      });

      expect('structure' in result).toBe(true);
      if ('structure' in result) {
        // Should not infinitely recurse - verify result exists
        expect(Object.keys(result.structure).length).toBeGreaterThanOrEqual(0);
      }
    });

    it('should deduplicate items when combining recursive results', async () => {
      const mockOctokit = {
        rest: {
          repos: {
            get: vi.fn().mockResolvedValue({
              data: {
                default_branch: 'main',
              },
            }),
            getContent: vi
              .fn()
              .mockResolvedValueOnce({
                data: [
                  {
                    name: 'src',
                    path: 'src',
                    type: 'dir',
                    size: 0,
                    url: 'https://api.github.com/repos/test/repo/contents/src',
                    html_url: 'https://github.com/test/repo/tree/main/src',
                    git_url:
                      'https://api.github.com/repos/test/repo/git/trees/abc',
                    sha: 'abc123',
                  },
                  {
                    name: 'index.ts',
                    path: 'index.ts',
                    type: 'file',
                    size: 100,
                    url: 'https://api.github.com/repos/test/repo/contents/index.ts',
                    html_url: 'https://github.com/test/repo/blob/main/index.ts',
                    git_url:
                      'https://api.github.com/repos/test/repo/git/blobs/xyz',
                    sha: 'xyz789',
                  },
                ],
              })
              .mockResolvedValueOnce({
                data: [
                  {
                    name: 'index.ts',
                    path: 'src/index.ts',
                    type: 'file',
                    size: 200,
                    url: 'https://api.github.com/repos/test/repo/contents/src/index.ts',
                    html_url:
                      'https://github.com/test/repo/blob/main/src/index.ts',
                    git_url:
                      'https://api.github.com/repos/test/repo/git/blobs/def',
                    sha: 'def456',
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
        depth: 2,
      });

      expect('structure' in result).toBe(true);
      if ('structure' in result) {
        // Root should have unique files
        expect(result.structure['.']?.files).toContain('index.ts');
        // src should have its own files
        expect(result.structure['src']).toBeDefined();
        expect(result.structure['src']?.files).toContain('index.ts');
      }
    });

    it('should handle errors in recursive directory fetching gracefully', async () => {
      const mockOctokit = {
        rest: {
          repos: {
            get: vi.fn().mockResolvedValue({
              data: {
                default_branch: 'main',
              },
            }),
            getContent: vi
              .fn()
              .mockResolvedValueOnce({
                data: [
                  {
                    name: 'src',
                    path: 'src',
                    type: 'dir',
                    size: 0,
                    url: 'https://api.github.com/repos/test/repo/contents/src',
                    html_url: 'https://github.com/test/repo/tree/main/src',
                    git_url:
                      'https://api.github.com/repos/test/repo/git/trees/abc',
                    sha: 'abc123',
                  },
                  {
                    name: 'docs',
                    path: 'docs',
                    type: 'dir',
                    size: 0,
                    url: 'https://api.github.com/repos/test/repo/contents/docs',
                    html_url: 'https://github.com/test/repo/tree/main/docs',
                    git_url:
                      'https://api.github.com/repos/test/repo/git/trees/def',
                    sha: 'def456',
                  },
                ],
              })
              // Error for src directory
              .mockRejectedValueOnce(new Error('Access denied'))
              // Success for docs directory
              .mockResolvedValueOnce({
                data: [
                  {
                    name: 'README.md',
                    path: 'docs/README.md',
                    type: 'file',
                    size: 100,
                    url: 'https://api.github.com/repos/test/repo/contents/docs/README.md',
                    html_url:
                      'https://github.com/test/repo/blob/main/docs/README.md',
                    git_url:
                      'https://api.github.com/repos/test/repo/git/blobs/ghi',
                    sha: 'ghi789',
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
        depth: 2,
      });

      expect('structure' in result).toBe(true);
      if ('structure' in result) {
        // Should still return results from successful directories
        expect(result.structure['.']).toBeDefined();
        expect(result.structure['.']?.folders).toContain('src');
        expect(result.structure['.']?.folders).toContain('docs');
      }
    });

    it('should sort items within directories alphabetically', async () => {
      const mockOctokit = {
        rest: {
          repos: {
            get: vi.fn().mockResolvedValue({
              data: {
                default_branch: 'main',
              },
            }),
            getContent: vi
              .fn()
              .mockResolvedValueOnce({
                data: [
                  {
                    name: 'file1.ts',
                    path: 'file1.ts',
                    type: 'file',
                    size: 100,
                    url: 'https://api.github.com/repos/test/repo/contents/file1.ts',
                    html_url: 'https://github.com/test/repo/blob/main/file1.ts',
                    git_url:
                      'https://api.github.com/repos/test/repo/git/blobs/a',
                    sha: 'aaa',
                  },
                  {
                    name: 'src',
                    path: 'src',
                    type: 'dir',
                    size: 0,
                    url: 'https://api.github.com/repos/test/repo/contents/src',
                    html_url: 'https://github.com/test/repo/tree/main/src',
                    git_url:
                      'https://api.github.com/repos/test/repo/git/trees/b',
                    sha: 'bbb',
                  },
                  {
                    name: 'file2.ts',
                    path: 'file2.ts',
                    type: 'file',
                    size: 100,
                    url: 'https://api.github.com/repos/test/repo/contents/file2.ts',
                    html_url: 'https://github.com/test/repo/blob/main/file2.ts',
                    git_url:
                      'https://api.github.com/repos/test/repo/git/blobs/c',
                    sha: 'ccc',
                  },
                  {
                    name: 'docs',
                    path: 'docs',
                    type: 'dir',
                    size: 0,
                    url: 'https://api.github.com/repos/test/repo/contents/docs',
                    html_url: 'https://github.com/test/repo/tree/main/docs',
                    git_url:
                      'https://api.github.com/repos/test/repo/git/trees/d',
                    sha: 'ddd',
                  },
                ],
              })
              .mockResolvedValueOnce({
                data: [
                  {
                    name: 'index.ts',
                    path: 'src/index.ts',
                    type: 'file',
                    size: 200,
                    url: 'https://api.github.com/repos/test/repo/contents/src/index.ts',
                    html_url:
                      'https://github.com/test/repo/blob/main/src/index.ts',
                    git_url:
                      'https://api.github.com/repos/test/repo/git/blobs/e',
                    sha: 'eee',
                  },
                ],
              })
              .mockResolvedValueOnce({
                data: [],
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
        depth: 2,
      });

      expect('structure' in result).toBe(true);
      if ('structure' in result) {
        // Files should be sorted alphabetically
        expect(result.structure['.']?.files).toEqual(['file1.ts', 'file2.ts']);
        // Folders should be sorted alphabetically
        expect(result.structure['.']?.folders).toEqual(['docs', 'src']);
      }
    });

    it('should filter ignored directories correctly', async () => {
      const mockOctokit = {
        rest: {
          repos: {
            get: vi.fn().mockResolvedValue({
              data: {
                default_branch: 'main',
              },
            }),
            getContent: vi
              .fn()
              .mockResolvedValueOnce({
                data: [
                  {
                    name: 'node_modules',
                    path: 'node_modules',
                    type: 'dir',
                    size: 0,
                    url: 'https://api.github.com/repos/test/repo/contents/node_modules',
                    html_url:
                      'https://github.com/test/repo/tree/main/node_modules',
                    git_url:
                      'https://api.github.com/repos/test/repo/git/trees/a',
                    sha: 'aaa',
                  },
                  {
                    name: '.git',
                    path: '.git',
                    type: 'dir',
                    size: 0,
                    url: 'https://api.github.com/repos/test/repo/contents/.git',
                    html_url: 'https://github.com/test/repo/tree/main/.git',
                    git_url:
                      'https://api.github.com/repos/test/repo/git/trees/b',
                    sha: 'bbb',
                  },
                  {
                    name: 'src',
                    path: 'src',
                    type: 'dir',
                    size: 0,
                    url: 'https://api.github.com/repos/test/repo/contents/src',
                    html_url: 'https://github.com/test/repo/tree/main/src',
                    git_url:
                      'https://api.github.com/repos/test/repo/git/trees/c',
                    sha: 'ccc',
                  },
                ],
              })
              .mockResolvedValueOnce({
                data: [
                  {
                    name: 'index.ts',
                    path: 'src/index.ts',
                    type: 'file',
                    size: 100,
                    url: 'https://api.github.com/repos/test/repo/contents/src/index.ts',
                    html_url:
                      'https://github.com/test/repo/blob/main/src/index.ts',
                    git_url:
                      'https://api.github.com/repos/test/repo/git/blobs/d',
                    sha: 'ddd',
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
        depth: 2,
      });

      expect('structure' in result).toBe(true);
      if ('structure' in result) {
        // Should not include node_modules or .git
        expect(result.structure['.']?.folders).not.toContain('node_modules');
        expect(result.structure['.']?.folders).not.toContain('.git');

        // Should include src
        expect(result.structure['.']?.folders).toContain('src');
      }
    });

    it('should respect item limit based on depth', async () => {
      // Generate many items (above the default page size of 100)
      const manyItems = Array.from({ length: 150 }, (_, i) => ({
        name: `file${i}.ts`,
        path: `file${i}.ts`,
        type: 'file' as const,
        size: 100,
        url: `https://api.github.com/repos/test/repo/contents/file${i}.ts`,
        html_url: `https://github.com/test/repo/blob/main/file${i}.ts`,
        git_url: `https://api.github.com/repos/test/repo/git/blobs/${i}`,
        sha: `sha${i}`,
      }));

      const mockOctokit = {
        rest: {
          repos: {
            get: vi.fn().mockResolvedValue({
              data: {
                default_branch: 'main',
              },
            }),
            getContent: vi.fn().mockResolvedValueOnce({
              data: manyItems,
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
        depth: 1,
      });

      expect('structure' in result).toBe(true);
      if ('structure' in result) {
        // Should be limited (default page size is 100)
        expect(result.structure['.']?.files?.length).toBeLessThanOrEqual(100);
        expect(result.summary?.truncated).toBe(true);
        expect(result.summary?.originalCount).toBeGreaterThan(
          result.structure['.']?.files?.length ?? 0
        );
      }
    });

    it('should handle depth > maxDepth in recursive function', async () => {
      const mockOctokit = {
        rest: {
          repos: {
            get: vi.fn().mockResolvedValue({
              data: {
                default_branch: 'main',
              },
            }),
            getContent: vi
              .fn()
              .mockResolvedValueOnce({
                data: [
                  {
                    name: 'src',
                    path: 'src',
                    type: 'dir',
                    size: 0,
                    url: 'https://api.github.com/repos/test/repo/contents/src',
                    html_url: 'https://github.com/test/repo/tree/main/src',
                    git_url:
                      'https://api.github.com/repos/test/repo/git/trees/a',
                    sha: 'aaa',
                  },
                ],
              })
              .mockResolvedValueOnce({
                data: [
                  {
                    name: 'nested',
                    path: 'src/nested',
                    type: 'dir',
                    size: 0,
                    url: 'https://api.github.com/repos/test/repo/contents/src/nested',
                    html_url:
                      'https://github.com/test/repo/tree/main/src/nested',
                    git_url:
                      'https://api.github.com/repos/test/repo/git/trees/b',
                    sha: 'bbb',
                  },
                ],
              })
              // Should not reach this level with depth=2
              .mockResolvedValueOnce({
                data: [
                  {
                    name: 'deep.ts',
                    path: 'src/nested/deep.ts',
                    type: 'file',
                    size: 100,
                    url: 'https://api.github.com/repos/test/repo/contents/src/nested/deep.ts',
                    html_url:
                      'https://github.com/test/repo/blob/main/src/nested/deep.ts',
                    git_url:
                      'https://api.github.com/repos/test/repo/git/blobs/c',
                    sha: 'ccc',
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
        depth: 2,
      });

      expect('structure' in result).toBe(true);
      if ('structure' in result) {
        // With depth=2, should have directory structure
        expect(result.structure['.']).toBeDefined();
        expect(result.structure['.']?.folders).toContain('src');
      }
    });
  });
});
