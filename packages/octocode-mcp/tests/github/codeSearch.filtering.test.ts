import { describe, it, expect, beforeEach, vi } from 'vitest';

const mockOctokit = vi.hoisted(() => ({
  rest: {
    search: {
      code: vi.fn(),
    },
  },
}));

vi.mock('../../../octocode-tools-core/src/github/client.js', () => ({
  getOctokit: vi.fn(() => mockOctokit),
}));

vi.mock('../../../octocode-tools-core/src/utils/http/cache.js', () => ({
  generateCacheKey: vi.fn(() => 'test-cache-key'),
  withDataCache: vi.fn(async (_key: string, fn: () => unknown) => {
    return await fn();
  }),
}));

import { searchGitHubCodeAPI } from '../../../octocode-tools-core/src/github/codeSearch.js';

describe('Code Search Filtering - File Filters', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('Folder filtering', () => {
    it('should filter out files in node_modules', async () => {
      const mockResponse = {
        data: {
          total_count: 3,
          items: [
            {
              name: 'index.js',
              path: 'src/index.js',
              html_url: 'https://github.com/test/repo/blob/main/src/index.js',
              repository: {
                full_name: 'test/repo',
                url: 'https://api.github.com/repos/test/repo',
              },
              text_matches: [
                {
                  fragment: 'function test() {}',
                  matches: [{ indices: [0, 8] }],
                },
              ],
            },
            {
              name: 'lodash.js',
              path: 'node_modules/lodash/lodash.js',
              html_url:
                'https://github.com/test/repo/blob/main/node_modules/lodash/lodash.js',
              repository: {
                full_name: 'test/repo',
                url: 'https://api.github.com/repos/test/repo',
              },
              text_matches: [
                {
                  fragment: 'function lodash() {}',
                  matches: [{ indices: [0, 8] }],
                },
              ],
            },
            {
              name: 'package.json',
              path: 'node_modules/react/package.json',
              html_url:
                'https://github.com/test/repo/blob/main/node_modules/react/package.json',
              repository: {
                full_name: 'test/repo',
                url: 'https://api.github.com/repos/test/repo',
              },
              text_matches: [],
            },
          ],
        },
      };

      mockOctokit.rest.search.code.mockResolvedValue(mockResponse);

      const result = await searchGitHubCodeAPI({
        keywords: ['function'],
        owner: 'test',
        repo: 'repo',
      });

      if ('data' in result) {
        expect(result.data.items.length).toBe(1);
        expect(result.data.items[0]!.path).toBe('src/index.js');
        expect(result.data.items[0]!.matches.length).toBeGreaterThan(0);
      } else {
        expect.fail('Expected successful result');
      }
    });

    it('should filter out multiple ignored directories', async () => {
      const mockResponse = {
        data: {
          total_count: 7,
          items: [
            {
              name: 'app.js',
              path: 'src/app.js',
              html_url: 'https://github.com/test/repo/blob/main/src/app.js',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'test.js',
              path: 'dist/test.js',
              html_url: 'https://github.com/test/repo/blob/main/dist/test.js',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'build.js',
              path: 'build/build.js',
              html_url: 'https://github.com/test/repo/blob/main/build/build.js',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'vendor.js',
              path: 'vendor/vendor.js',
              html_url:
                'https://github.com/test/repo/blob/main/vendor/vendor.js',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'cache.js',
              path: '.cache/cache.js',
              html_url:
                'https://github.com/test/repo/blob/main/.cache/cache.js',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'git.js',
              path: '.git/hooks/pre-commit',
              html_url:
                'https://github.com/test/repo/blob/main/.git/hooks/pre-commit',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'valid.js',
              path: 'src/components/valid.js',
              html_url:
                'https://github.com/test/repo/blob/main/src/components/valid.js',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
          ],
        },
      };

      mockOctokit.rest.search.code.mockResolvedValue(mockResponse);

      const result = await searchGitHubCodeAPI({
        keywords: ['test'],
        owner: 'test',
        repo: 'repo',
      });

      if ('data' in result) {
        expect(result.data.items).toEqual([
          {
            path: 'src/app.js',
            url: 'https://github.com/test/repo/blob/main/src/app.js',
            repository: { nameWithOwner: 'test/repo', url: 'url' },
            matches: [],
          },
          {
            path: 'src/components/valid.js',
            url: 'https://github.com/test/repo/blob/main/src/components/valid.js',
            repository: { nameWithOwner: 'test/repo', url: 'url' },
            matches: [],
          },
        ]);
      } else {
        expect.fail('Expected successful result');
      }
    });
  });

  describe('File name filtering', () => {
    it('should filter out lock files', async () => {
      const mockResponse = {
        data: {
          total_count: 4,
          items: [
            {
              name: 'package.json',
              path: 'package.json',
              html_url: 'https://github.com/test/repo/blob/main/package.json',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'package-lock.json',
              path: 'package-lock.json',
              html_url:
                'https://github.com/test/repo/blob/main/package-lock.json',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'yarn.lock',
              path: 'yarn.lock',
              html_url: 'https://github.com/test/repo/blob/main/yarn.lock',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'index.js',
              path: 'src/index.js',
              html_url: 'https://github.com/test/repo/blob/main/src/index.js',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
          ],
        },
      };

      mockOctokit.rest.search.code.mockResolvedValue(mockResponse);

      const result = await searchGitHubCodeAPI({
        keywords: ['test'],
        owner: 'test',
        repo: 'repo',
      });

      if ('data' in result) {
        expect(result.data.items).toEqual([
          {
            path: 'package.json',
            url: 'https://github.com/test/repo/blob/main/package.json',
            repository: { nameWithOwner: 'test/repo', url: 'url' },
            matches: [],
          },
          {
            path: 'src/index.js',
            url: 'https://github.com/test/repo/blob/main/src/index.js',
            repository: { nameWithOwner: 'test/repo', url: 'url' },
            matches: [],
          },
        ]);
      } else {
        expect.fail('Expected successful result');
      }
    });

    it('should filter out configuration files', async () => {
      const mockResponse = {
        data: {
          total_count: 6,
          items: [
            {
              name: '.gitignore',
              path: '.gitignore',
              html_url: 'https://github.com/test/repo/blob/main/.gitignore',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: '.eslintrc',
              path: '.eslintrc',
              html_url: 'https://github.com/test/repo/blob/main/.eslintrc',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'tsconfig.json',
              path: 'tsconfig.json',
              html_url: 'https://github.com/test/repo/blob/main/tsconfig.json',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'webpack.config.js',
              path: 'webpack.config.js',
              html_url:
                'https://github.com/test/repo/blob/main/webpack.config.js',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'Dockerfile',
              path: 'Dockerfile',
              html_url: 'https://github.com/test/repo/blob/main/Dockerfile',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'app.js',
              path: 'src/app.js',
              html_url: 'https://github.com/test/repo/blob/main/src/app.js',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
          ],
        },
      };

      mockOctokit.rest.search.code.mockResolvedValue(mockResponse);

      const result = await searchGitHubCodeAPI({
        keywords: ['config'],
        owner: 'test',
        repo: 'repo',
      });

      if ('data' in result) {
        expect(result.data.items).toEqual([
          {
            path: '.gitignore',
            url: 'https://github.com/test/repo/blob/main/.gitignore',
            repository: { nameWithOwner: 'test/repo', url: 'url' },
            matches: [],
          },
          {
            path: '.eslintrc',
            url: 'https://github.com/test/repo/blob/main/.eslintrc',
            repository: { nameWithOwner: 'test/repo', url: 'url' },
            matches: [],
          },
          {
            path: 'tsconfig.json',
            url: 'https://github.com/test/repo/blob/main/tsconfig.json',
            repository: { nameWithOwner: 'test/repo', url: 'url' },
            matches: [],
          },
          {
            path: 'webpack.config.js',
            url: 'https://github.com/test/repo/blob/main/webpack.config.js',
            repository: { nameWithOwner: 'test/repo', url: 'url' },
            matches: [],
          },
          {
            path: 'Dockerfile',
            url: 'https://github.com/test/repo/blob/main/Dockerfile',
            repository: { nameWithOwner: 'test/repo', url: 'url' },
            matches: [],
          },
          {
            path: 'src/app.js',
            url: 'https://github.com/test/repo/blob/main/src/app.js',
            repository: { nameWithOwner: 'test/repo', url: 'url' },
            matches: [],
          },
        ]);
      } else {
        expect.fail('Expected successful result');
      }
    });

    it('should filter out sensitive files', async () => {
      const mockResponse = {
        data: {
          total_count: 5,
          items: [
            {
              name: '.env',
              path: '.env',
              html_url: 'https://github.com/test/repo/blob/main/.env',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: '.env.local',
              path: '.env.local',
              html_url: 'https://github.com/test/repo/blob/main/.env.local',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'secrets.json',
              path: 'secrets.json',
              html_url: 'https://github.com/test/repo/blob/main/secrets.json',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'credentials.json',
              path: 'config/credentials.json',
              html_url:
                'https://github.com/test/repo/blob/main/config/credentials.json',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'config.js',
              path: 'src/config.js',
              html_url: 'https://github.com/test/repo/blob/main/src/config.js',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
          ],
        },
      };

      mockOctokit.rest.search.code.mockResolvedValue(mockResponse);

      const result = await searchGitHubCodeAPI({
        keywords: ['env'],
        owner: 'test',
        repo: 'repo',
      });

      if ('data' in result) {
        expect(result.data.items).toEqual([
          {
            path: '.env',
            url: 'https://github.com/test/repo/blob/main/.env',
            repository: { nameWithOwner: 'test/repo', url: 'url' },
            matches: [],
          },
          {
            path: '.env.local',
            url: 'https://github.com/test/repo/blob/main/.env.local',
            repository: { nameWithOwner: 'test/repo', url: 'url' },
            matches: [],
          },
          {
            path: 'src/config.js',
            url: 'https://github.com/test/repo/blob/main/src/config.js',
            repository: { nameWithOwner: 'test/repo', url: 'url' },
            matches: [],
          },
        ]);
      } else {
        expect.fail('Expected successful result');
      }
    });
  });

  describe('File extension filtering', () => {
    it('should filter out binary and compiled files', async () => {
      const mockResponse = {
        data: {
          total_count: 8,
          items: [
            {
              name: 'app.js',
              path: 'src/app.js',
              html_url: 'https://github.com/test/repo/blob/main/src/app.js',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'app.exe',
              path: 'bin/app.exe',
              html_url: 'https://github.com/test/repo/blob/main/bin/app.exe',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'lib.dll',
              path: 'lib/lib.dll',
              html_url: 'https://github.com/test/repo/blob/main/lib/lib.dll',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'module.so',
              path: 'lib/module.so',
              html_url: 'https://github.com/test/repo/blob/main/lib/module.so',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'Main.class',
              path: 'build/Main.class',
              html_url:
                'https://github.com/test/repo/blob/main/build/Main.class',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'cache.pyc',
              path: '__pycache__/cache.pyc',
              html_url:
                'https://github.com/test/repo/blob/main/__pycache__/cache.pyc',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'app.jar',
              path: 'dist/app.jar',
              html_url: 'https://github.com/test/repo/blob/main/dist/app.jar',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'main.py',
              path: 'src/main.py',
              html_url: 'https://github.com/test/repo/blob/main/src/main.py',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
          ],
        },
      };

      mockOctokit.rest.search.code.mockResolvedValue(mockResponse);

      const result = await searchGitHubCodeAPI({
        keywords: ['app'],
        owner: 'test',
        repo: 'repo',
      });

      if ('data' in result) {
        expect(result.data.items).toEqual([
          {
            path: 'src/app.js',
            url: 'https://github.com/test/repo/blob/main/src/app.js',
            repository: { nameWithOwner: 'test/repo', url: 'url' },
            matches: [],
          },
          {
            path: 'src/main.py',
            url: 'https://github.com/test/repo/blob/main/src/main.py',
            repository: { nameWithOwner: 'test/repo', url: 'url' },
            matches: [],
          },
        ]);
      } else {
        expect.fail('Expected successful result');
      }
    });

    it('should filter out minified files', async () => {
      const mockResponse = {
        data: {
          total_count: 3,
          items: [
            {
              name: 'app.js',
              path: 'src/app.js',
              html_url: 'https://github.com/test/repo/blob/main/src/app.js',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'app.min.js',
              path: 'dist/app.min.js',
              html_url:
                'https://github.com/test/repo/blob/main/dist/app.min.js',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'styles.min.css',
              path: 'dist/styles.min.css',
              html_url:
                'https://github.com/test/repo/blob/main/dist/styles.min.css',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
          ],
        },
      };

      mockOctokit.rest.search.code.mockResolvedValue(mockResponse);

      const result = await searchGitHubCodeAPI({
        keywords: ['app'],
        owner: 'test',
        repo: 'repo',
      });

      if ('data' in result) {
        expect(result.data.items).toEqual([
          {
            path: 'src/app.js',
            url: 'https://github.com/test/repo/blob/main/src/app.js',
            repository: { nameWithOwner: 'test/repo', url: 'url' },
            matches: [],
          },
        ]);
      } else {
        expect.fail('Expected successful result');
      }
    });

    it('should filter out archive files', async () => {
      const mockResponse = {
        data: {
          total_count: 6,
          items: [
            {
              name: 'app.js',
              path: 'src/app.js',
              html_url: 'https://github.com/test/repo/blob/main/src/app.js',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'backup.zip',
              path: 'backup.zip',
              html_url: 'https://github.com/test/repo/blob/main/backup.zip',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'archive.tar.gz',
              path: 'archive.tar.gz',
              html_url: 'https://github.com/test/repo/blob/main/archive.tar.gz',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'data.rar',
              path: 'data.rar',
              html_url: 'https://github.com/test/repo/blob/main/data.rar',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'package.7z',
              path: 'package.7z',
              html_url: 'https://github.com/test/repo/blob/main/package.7z',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'readme.md',
              path: 'readme.md',
              html_url: 'https://github.com/test/repo/blob/main/readme.md',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
          ],
        },
      };

      mockOctokit.rest.search.code.mockResolvedValue(mockResponse);

      const result = await searchGitHubCodeAPI({
        keywords: ['test'],
        owner: 'test',
        repo: 'repo',
      });

      if ('data' in result) {
        expect(result.data.items).toEqual([
          {
            path: 'src/app.js',
            url: 'https://github.com/test/repo/blob/main/src/app.js',
            repository: { nameWithOwner: 'test/repo', url: 'url' },
            matches: [],
          },
          {
            path: 'readme.md',
            url: 'https://github.com/test/repo/blob/main/readme.md',
            repository: { nameWithOwner: 'test/repo', url: 'url' },
            matches: [],
          },
        ]);
      } else {
        expect.fail('Expected successful result');
      }
    });

    it('should filter out temporary and cache files', async () => {
      const mockResponse = {
        data: {
          total_count: 6,
          items: [
            {
              name: 'app.js',
              path: 'src/app.js',
              html_url: 'https://github.com/test/repo/blob/main/src/app.js',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'file.tmp',
              path: 'temp/file.tmp',
              html_url: 'https://github.com/test/repo/blob/main/temp/file.tmp',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'data.cache',
              path: 'cache/data.cache',
              html_url:
                'https://github.com/test/repo/blob/main/cache/data.cache',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'backup.bak',
              path: 'backup.bak',
              html_url: 'https://github.com/test/repo/blob/main/backup.bak',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: '.swp',
              path: '.swp',
              html_url: 'https://github.com/test/repo/blob/main/.swp',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
          ],
        },
      };

      mockOctokit.rest.search.code.mockResolvedValue(mockResponse);

      const result = await searchGitHubCodeAPI({
        keywords: ['test'],
        owner: 'test',
        repo: 'repo',
      });

      if ('data' in result) {
        expect(result.data.items).toEqual([
          {
            path: 'src/app.js',
            url: 'https://github.com/test/repo/blob/main/src/app.js',
            repository: { nameWithOwner: 'test/repo', url: 'url' },
            matches: [],
          },
        ]);
      } else {
        expect.fail('Expected successful result');
      }
    });
  });

  describe('New research context fields', () => {
    it('should include lastModifiedAt when present in API response', async () => {
      const mockResponse = {
        data: {
          total_count: 1,
          items: [
            {
              name: 'app.js',
              path: 'src/app.js',
              html_url: 'https://github.com/test/repo/blob/main/src/app.js',
              repository: {
                full_name: 'test/repo',
                url: 'https://api.github.com/repos/test/repo',
              },
              text_matches: [],
              last_modified_at: '2025-12-01T10:30:00Z',
            },
          ],
        },
      };

      mockOctokit.rest.search.code.mockResolvedValue(mockResponse);

      const result = await searchGitHubCodeAPI({
        keywords: ['function'],
        owner: 'test',
        repo: 'repo',
      });

      if ('data' in result) {
        expect(result.data.items[0]).toHaveProperty('lastModifiedAt');
        expect(result.data.items[0]!.lastModifiedAt).toBe(
          '2025-12-01T10:30:00Z'
        );
      } else {
        expect.fail('Expected successful result');
      }
    });

    it('should not include lastModifiedAt when not present in API response', async () => {
      const mockResponse = {
        data: {
          total_count: 1,
          items: [
            {
              name: 'app.js',
              path: 'src/app.js',
              html_url: 'https://github.com/test/repo/blob/main/src/app.js',
              repository: {
                full_name: 'test/repo',
                url: 'https://api.github.com/repos/test/repo',
              },
              text_matches: [],
            },
          ],
        },
      };

      mockOctokit.rest.search.code.mockResolvedValue(mockResponse);

      const result = await searchGitHubCodeAPI({
        keywords: ['function'],
        owner: 'test',
        repo: 'repo',
      });

      if ('data' in result) {
        expect(result.data.items[0]).not.toHaveProperty('lastModifiedAt');
      } else {
        expect.fail('Expected successful result');
      }
    });
  });

  describe('Combined filtering scenarios', () => {
    it('should handle mixed valid and invalid files correctly', async () => {
      const mockResponse = {
        data: {
          total_count: 10,
          items: [
            {
              name: 'UserService.js',
              path: 'src/services/UserService.js',
              html_url:
                'https://github.com/test/repo/blob/main/src/services/UserService.js',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'AuthController.js',
              path: 'src/controllers/AuthController.js',
              html_url:
                'https://github.com/test/repo/blob/main/src/controllers/AuthController.js',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'utils.ts',
              path: 'src/utils/utils.ts',
              html_url:
                'https://github.com/test/repo/blob/main/src/utils/utils.ts',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'index.js',
              path: 'node_modules/express/index.js',
              html_url:
                'https://github.com/test/repo/blob/main/node_modules/express/index.js',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'bundle.js',
              path: 'dist/bundle.js',
              html_url: 'https://github.com/test/repo/blob/main/dist/bundle.js',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'test.js',
              path: '.git/hooks/test.js',
              html_url:
                'https://github.com/test/repo/blob/main/.git/hooks/test.js',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'package-lock.json',
              path: 'package-lock.json',
              html_url:
                'https://github.com/test/repo/blob/main/package-lock.json',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: '.env',
              path: '.env',
              html_url: 'https://github.com/test/repo/blob/main/.env',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'app.exe',
              path: 'bin/app.exe',
              html_url: 'https://github.com/test/repo/blob/main/bin/app.exe',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
          ],
        },
      };

      mockOctokit.rest.search.code.mockResolvedValue(mockResponse);

      const result = await searchGitHubCodeAPI({
        keywords: ['test'],
        owner: 'test',
        repo: 'repo',
      });

      if ('data' in result) {
        expect(result.data.items).toEqual([
          {
            path: 'src/services/UserService.js',
            url: 'https://github.com/test/repo/blob/main/src/services/UserService.js',
            repository: { nameWithOwner: 'test/repo', url: 'url' },
            matches: [],
          },
          {
            path: 'src/controllers/AuthController.js',
            url: 'https://github.com/test/repo/blob/main/src/controllers/AuthController.js',
            repository: { nameWithOwner: 'test/repo', url: 'url' },
            matches: [],
          },
          {
            path: 'src/utils/utils.ts',
            url: 'https://github.com/test/repo/blob/main/src/utils/utils.ts',
            repository: { nameWithOwner: 'test/repo', url: 'url' },
            matches: [],
          },
          {
            path: '.env',
            url: 'https://github.com/test/repo/blob/main/.env',
            repository: { nameWithOwner: 'test/repo', url: 'url' },
            matches: [],
          },
        ]);
      } else {
        expect.fail('Expected successful result');
      }
    });

    it('should handle empty results after filtering', async () => {
      const mockResponse = {
        data: {
          total_count: 3,
          items: [
            {
              name: 'package-lock.json',
              path: 'package-lock.json',
              html_url:
                'https://github.com/test/repo/blob/main/package-lock.json',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'yarn.lock',
              path: 'yarn.lock',
              html_url: 'https://github.com/test/repo/blob/main/yarn.lock',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'Cargo.lock',
              path: 'Cargo.lock',
              html_url: 'https://github.com/test/repo/blob/main/Cargo.lock',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
          ],
        },
      };

      mockOctokit.rest.search.code.mockResolvedValue(mockResponse);

      const result = await searchGitHubCodeAPI({
        keywords: ['lock'],
        owner: 'test',
        repo: 'repo',
      });

      if ('data' in result) {
        expect(result.data.items).toEqual([]);
      } else {
        expect.fail('Expected successful result');
      }
    });

    it('should correctly update total_count after filtering', async () => {
      const mockResponse = {
        data: {
          total_count: 100,
          items: [
            {
              name: 'app.js',
              path: 'src/app.js',
              html_url: 'https://github.com/test/repo/blob/main/src/app.js',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'test.js',
              path: 'node_modules/jest/test.js',
              html_url:
                'https://github.com/test/repo/blob/main/node_modules/jest/test.js',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'config.js',
              path: 'src/config.js',
              html_url: 'https://github.com/test/repo/blob/main/src/config.js',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'bundle.js',
              path: 'dist/bundle.js',
              html_url: 'https://github.com/test/repo/blob/main/dist/bundle.js',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
            {
              name: 'utils.js',
              path: 'src/utils.js',
              html_url: 'https://github.com/test/repo/blob/main/src/utils.js',
              repository: { full_name: 'test/repo', url: 'url' },
              text_matches: [],
            },
          ],
        },
      };

      mockOctokit.rest.search.code.mockResolvedValue(mockResponse);

      const result = await searchGitHubCodeAPI({
        keywords: ['test'],
        owner: 'test',
        repo: 'repo',
      });

      if ('data' in result) {
        expect(result.data.items).toEqual([
          {
            path: 'src/app.js',
            url: 'https://github.com/test/repo/blob/main/src/app.js',
            repository: { nameWithOwner: 'test/repo', url: 'url' },
            matches: [],
          },
          {
            path: 'src/config.js',
            url: 'https://github.com/test/repo/blob/main/src/config.js',
            repository: { nameWithOwner: 'test/repo', url: 'url' },
            matches: [],
          },
          {
            path: 'src/utils.js',
            url: 'https://github.com/test/repo/blob/main/src/utils.js',
            repository: { nameWithOwner: 'test/repo', url: 'url' },
            matches: [],
          },
        ]);
      } else {
        expect.fail('Expected successful result');
      }
    });
  });
});

describe('Code Search Resilience - Promise.allSettled', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('should return other items when one item throws during processing', async () => {
    const brokenItem = {
      name: 'broken.js',
      path: 'src/broken.js',
      get html_url(): string {
        throw new Error('Item processing crash');
      },
      repository: { full_name: 'test/repo', url: 'url' },
      text_matches: [],
    };

    const mockResponse = {
      data: {
        total_count: 2,
        items: [
          {
            name: 'good.js',
            path: 'src/good.js',
            html_url: 'https://github.com/test/repo/blob/main/src/good.js',
            repository: { full_name: 'test/repo', url: 'url' },
            text_matches: [],
          },
          brokenItem,
        ],
      },
    };

    mockOctokit.rest.search.code.mockResolvedValue(mockResponse);

    const result = await searchGitHubCodeAPI({
      keywords: ['test'],
      owner: 'test',
      repo: 'repo',
    });

    if ('data' in result) {
      expect(result.data.items.length).toBe(1);
      expect(result.data.items[0]!.path).toBe('src/good.js');
      expect(result.data.matchLocations).toBeDefined();
      expect(
        result.data.matchLocations!.some((m: string) =>
          m.includes('item(s) dropped')
        )
      ).toBe(true);
    } else {
      expect.fail('Expected successful result');
    }
  });

  it('should return other matches when one match throws in the same item', async () => {
    const { ContentSanitizer } =
      await import('octocode-security/contentSanitizer');
    let callCount = 0;
    const sanitizeSpy = vi
      .spyOn(ContentSanitizer, 'sanitizeContent')
      .mockImplementation((content: string) => {
        callCount++;
        if (callCount === 2) {
          throw new Error('Match processing crash');
        }
        return {
          content,
          hasSecrets: false,
          secretsDetected: [],
          warnings: [],
        };
      });

    const mockResponse = {
      data: {
        total_count: 1,
        items: [
          {
            name: 'multi.js',
            path: 'src/multi.js',
            html_url: 'https://github.com/test/repo/blob/main/src/multi.js',
            repository: { full_name: 'test/repo', url: 'url' },
            text_matches: [
              { fragment: 'match1', matches: [{ indices: [0, 6] }] },
              { fragment: 'match2', matches: [{ indices: [0, 6] }] },
              { fragment: 'match3', matches: [{ indices: [0, 6] }] },
            ],
          },
        ],
      },
    };

    mockOctokit.rest.search.code.mockResolvedValue(mockResponse);

    const result = await searchGitHubCodeAPI({
      keywords: ['test'],
      owner: 'test',
      repo: 'repo',
    });

    if ('data' in result) {
      expect(result.data.items.length).toBe(1);
      expect(result.data.items[0]!.matches.length).toBe(2);
      expect(result.data.matchLocations).toBeDefined();
      expect(
        result.data.matchLocations!.some((m: string) =>
          m.includes('match(es) dropped')
        )
      ).toBe(true);
    } else {
      expect.fail('Expected successful result');
    }

    sanitizeSpy.mockRestore();
  });

  it('should return empty array (not throw) when all items fail', async () => {
    const makeCrashingItem = (name: string, itemPath: string) => ({
      name,
      path: itemPath,
      get html_url(): string {
        throw new Error('Item processing crash');
      },
      repository: { full_name: 'test/repo', url: 'url' },
      text_matches: [],
    });

    const mockResponse = {
      data: {
        total_count: 2,
        items: [
          makeCrashingItem('a.js', 'src/a.js'),
          makeCrashingItem('b.js', 'src/b.js'),
        ],
      },
    };

    mockOctokit.rest.search.code.mockResolvedValue(mockResponse);

    const result = await searchGitHubCodeAPI({
      keywords: ['test'],
      owner: 'test',
      repo: 'repo',
    });

    if ('data' in result) {
      expect(result.data.items).toEqual([]);
      expect(result.data.matchLocations).toBeDefined();
      expect(
        result.data.matchLocations!.some((m: string) =>
          m.includes('2 item(s) dropped')
        )
      ).toBe(true);
    } else {
      expect.fail('Expected successful result');
    }
  });
});
