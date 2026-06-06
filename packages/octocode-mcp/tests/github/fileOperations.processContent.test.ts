import { describe, it, expect, beforeEach, vi } from 'vitest';
import { fetchGitHubFileContentAPI } from '../../src/github/fileContent.js';
import { viewGitHubRepositoryStructureAPI } from '../../src/github/repoStructure.js';
import { getOctokit, resolveDefaultBranch } from '../../src/github/client.js';
import { RequestError } from 'octokit';
import * as minifierModule from '../../src/utils/minifier/minifier.js';
import { clearAllCache } from '../../src/utils/http/cache.js';

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

vi.mock('../../src/github/client.js');
vi.mock('../../src/utils/minifier/minifier.js');

describe('GitHub File Operations - processFileContentAPI coverage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    clearAllCache();
    vi.mocked(resolveDefaultBranch).mockResolvedValue('main');
  });

  describe('fetchGitHubFileContentAPI - File Size and Encoding', () => {
    it('should reject files larger than 300KB', async () => {
      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi.fn().mockResolvedValue({
              data: {
                type: 'file',
                content: Buffer.from('test').toString('base64'),
                size: 400 * 1024,
                sha: 'abc123',
                name: 'large-file.txt',
                path: 'large-file.txt',
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
        path: 'large-file.txt',
      });

      expect('error' in result).toBe(true);
      if ('error' in result) {
        expect(result.error).toContain('File too large');
        expect(result.error).toContain('400KB');
        expect(result.error).toContain('300KB');
        expect(result.status).toBe(413);
      }
    });

    it('should handle files at exactly 300KB (boundary)', async () => {
      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi.fn().mockResolvedValue({
              data: {
                type: 'file',
                content: Buffer.from('x'.repeat(300 * 1024)).toString('base64'),
                size: 300 * 1024,
                sha: 'abc123',
                name: 'boundary.txt',
                path: 'boundary.txt',
              },
            }),
          },
        },
      };

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );
      vi.mocked(minifierModule.minifyContent).mockResolvedValue({
        content: 'minified',
        failed: false,
        type: 'general',
      });

      const result = await fetchGitHubFileContentAPI({
        owner: 'test',
        repo: 'repo',
        path: 'boundary.txt',
      });

      expect(result).toHaveProperty('data');
      expect('error' in result).toBe(false);
    });

    it('should detect and reject binary files', async () => {
      const binaryBuffer = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x00, 0x00]);

      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi.fn().mockResolvedValue({
              data: {
                type: 'file',
                content: binaryBuffer.toString('base64'),
                size: 100,
                sha: 'abc123',
                name: 'image.png',
                path: 'image.png',
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
        path: 'image.png',
      });

      expect('error' in result).toBe(true);
      if ('error' in result) {
        expect(result.error).toContain('Binary file detected');
        expect(result.status).toBe(415);
      }
    });

    it('should handle empty file content', async () => {
      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi.fn().mockResolvedValue({
              data: {
                type: 'file',
                content: '',
                size: 0,
                sha: 'abc123',
                name: 'empty.txt',
                path: 'empty.txt',
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
        path: 'empty.txt',
      });

      expect('error' in result).toBe(true);
      if ('error' in result) {
        expect(result.error).toContain('File is empty');
        expect(result.status).toBe(404);
      }
    });

    it('should handle whitespace-only base64 content', async () => {
      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi.fn().mockResolvedValue({
              data: {
                type: 'file',
                content: '   \n  \t  ',
                size: 10,
                sha: 'abc123',
                name: 'whitespace.txt',
                path: 'whitespace.txt',
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
        path: 'whitespace.txt',
      });

      expect('error' in result).toBe(true);
      if ('error' in result) {
        expect(result.error).toContain('File is empty');
      }
    });

    it('should handle encoding/decoding errors', async () => {
      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi.fn().mockResolvedValue({
              data: {
                type: 'file',
                content: 'invalid!!!base64',
                size: 100,
                sha: 'abc123',
                name: 'invalid.txt',
                path: 'invalid.txt',
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
        path: 'invalid.txt',
      });

      if ('error' in result) {
        expect(result.error).toBeTruthy();
      }
    });

    it('should reject directory paths', async () => {
      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi.fn().mockResolvedValue({
              data: [
                { name: 'file1.txt', type: 'file' },
                { name: 'file2.txt', type: 'file' },
              ],
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
        path: 'directory',
      });

      expect('error' in result).toBe(true);
      if ('error' in result) {
        expect(result.error).toContain('Path is a directory');
        expect(typeof result.error).toBe('string');
        expect(result.error.length).toBeGreaterThan(0);
        expect(result.status).toBe(400);
      }
    });

    it('should reject unsupported file types (symlinks)', async () => {
      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi.fn().mockResolvedValue({
              data: {
                type: 'symlink',
                target: 'target.txt',
                name: 'link.txt',
                path: 'link.txt',
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
        path: 'link.txt',
      });

      expect('error' in result).toBe(true);
      if ('error' in result) {
        expect(result.error).toContain('Unsupported file type');
        expect(result.status).toBe(415);
      }
    });
  });

  describe('fetchGitHubFileContentAPI - Match String Not Found', () => {
    it('should return success with matchNotFound flag when matchString not found in file', async () => {
      const fileContent = 'Line 1\nLine 2\nLine 3\nLine 4';

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
        matchString: 'NonExistentString',
      });

      expect(result).toHaveProperty('status', 200);
      expect('data' in result).toBe(true);
      if ('data' in result && result.data) {
        expect(result.data.matchNotFound).toBe(true);
        expect(result.data.searchedFor).toBe('NonExistentString');
        expect(result.data.content).toBe('');
        expect(result.data.hints).toBeDefined();
        expect(result.data.hints?.[0]).toContain('not found');
      }
    });

    it('should find matchString case-insensitively', async () => {
      const fileContent = 'Line 1\nTarget Line\nLine 3\nLine 4\nLine 5';

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
        path: 'test.txt',
        matchString: 'TARGET LINE',
        matchStringContextLines: 1,
      });

      expect(result.status).toBe(200);
      expect('data' in result).toBe(true);
      if ('data' in result) {
        expect(result.data.matchNotFound).toBeUndefined();
        expect(result.data.content).toContain('Target Line');
      }
    });

    it('should resolve a whitespace-stripped (minified) anchor against the raw line (FC-1)', async () => {
      const fileContent =
        'Line 1\nattachPingListener(root, wakeable, rootRenderLanes)\nLine 3';

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
          },
        },
      };

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );
      vi.mocked(minifierModule.minifyContent).mockImplementation(
        async content => ({ content, failed: false, type: 'general' })
      );

      const result = await fetchGitHubFileContentAPI({
        owner: 'test',
        repo: 'repo',
        path: 'test.txt',
        matchString: 'attachPingListener(root,wakeable,rootRenderLanes)',
        matchStringContextLines: 0,
      });

      expect(result.status).toBe(200);
      expect('data' in result).toBe(true);
      if ('data' in result) {
        expect(result.data.matchNotFound).toBeUndefined();
        expect(result.data.content).toContain('attachPingListener');
      }
    });

    it('should find matchString and include it in response', async () => {
      const fileContent = 'Line 1\nTarget Line\nLine 3\nLine 4\nLine 5';

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
        path: 'test.txt',
        matchString: 'Target Line',
        matchStringContextLines: 1,
      });

      expect(result).toHaveProperty('data');
      if ('data' in result && !('error' in result.data)) {
        expect(result.data.isPartial).toBe(true);
        expect(result.data.startLine).toBeLessThanOrEqual(2);
        expect(result.data.endLine).toBeGreaterThanOrEqual(2);
        expect(result.data.matchLocations).toBeDefined();
        expect(
          result.data.matchLocations?.some(w =>
            w.includes('Found "Target Line"')
          )
        ).toBe(true);
      }
    });
  });

  describe('fetchGitHubFileContentAPI - Line Range Edge Cases', () => {
    it('should handle invalid startLine (< 1)', async () => {
      const fileContent = 'Line 1\nLine 2\nLine 3';

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
        path: 'test.txt',
        startLine: -5,
        endLine: 2,
      });

      expect(result).toHaveProperty('data');
      if ('data' in result && !('error' in result.data)) {
        expect(result.data.content).toContain('Line 1');
        expect(result.data.content).toContain('Line 3');
      }
    });

    it('should handle startLine > totalLines', async () => {
      const fileContent = 'Line 1\nLine 2\nLine 3';

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
        path: 'test.txt',
        startLine: 100,
        endLine: 200,
      });

      expect(result).toHaveProperty('data');
      if ('data' in result && !('error' in result.data)) {
        expect(result.data.content).toBeTruthy();
      }
    });

    it('should handle endLine < startLine', async () => {
      const fileContent = 'Line 1\nLine 2\nLine 3';

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
        path: 'test.txt',
        startLine: 5,
        endLine: 2,
      });

      expect(result).toHaveProperty('data');
      if ('data' in result && !('error' in result.data)) {
        expect(result.data.content).toBeTruthy();
      }
    });

    it('should adjust endLine when it exceeds totalLines', async () => {
      const fileContent = 'Line 1\nLine 2\nLine 3';

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
        path: 'test.txt',
        startLine: 1,
        endLine: 100,
      });

      expect(result).toHaveProperty('data');
      if ('data' in result && !('error' in result.data)) {
        expect(result.data.isPartial).toBe(true);
        expect(result.data.endLine).toBe(3);
        expect(result.data.matchLocations).toBeDefined();
        expect(
          result.data.matchLocations?.some(w => w.includes('adjusted to 3'))
        ).toBe(true);
      }
    });

    it('should handle only endLine provided (defaults startLine to 1)', async () => {
      const fileContent = 'Line 1\nLine 2\nLine 3\nLine 4';

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
        path: 'test.txt',
        endLine: 2,
      });

      expect(result).toHaveProperty('data');
      if ('data' in result && !('error' in result.data)) {
        expect(result.data.isPartial).toBe(true);
        expect(result.data.startLine).toBe(1);
        expect(result.data.endLine).toBe(2);
      }
    });
  });

  describe('fetchGitHubFileContentAPI - Minification Edge Cases', () => {
    it('should handle minification failure gracefully', async () => {
      const fileContent = 'Test content';

      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi.fn().mockResolvedValue({
              data: {
                type: 'file',
                content: Buffer.from(fileContent).toString('base64'),
                size: fileContent.length,
                sha: 'abc123',
                name: 'test.js',
                path: 'test.js',
              },
            }),
          },
        },
      };

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );
      vi.mocked(minifierModule.minifyContent).mockResolvedValue({
        content: fileContent,
        failed: true,
        type: 'terser',
      });

      const result = await fetchGitHubFileContentAPI({
        owner: 'test',
        repo: 'repo',
        path: 'test.js',
      });

      expect(result).toHaveProperty('data');
    });
  });

  describe('fetchGitHubFileContentAPI - content is verbatim (no pre-finalizer minify)', () => {
    it('does NOT minify fullContent in the base processor', async () => {
      const fileContent = '{\n  "name": "demo",\n  "version": "1.0.0"\n}';

      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi.fn().mockResolvedValue({
              data: {
                type: 'file',
                content: Buffer.from(fileContent).toString('base64'),
                size: fileContent.length,
                sha: 'abc123',
                name: 'package.json',
                path: 'package.json',
              },
            }),
          },
        },
      };

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );
      const minifySpy = vi
        .mocked(minifierModule.minifyContent)
        .mockResolvedValue({
          content: 'SHOULD_NOT_APPEAR',
          failed: false,
          type: 'json',
        });

      const result = await fetchGitHubFileContentAPI({
        owner: 'test',
        repo: 'repo',
        path: 'package.json',
        fullContent: true,
      });

      expect('data' in result).toBe(true);
      if ('data' in result && result.data) {
        expect(result.data.content).toBe(fileContent);
      }
      expect(minifySpy).not.toHaveBeenCalled();
    });
  });

  describe('viewGitHubRepositoryStructureAPI - Branch Fallback', () => {
    it('should not try default branch when requested branch fails', async () => {
      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi
              .fn()
              .mockRejectedValueOnce(createRequestError('Not Found', 404)),
            get: vi.fn().mockResolvedValue({
              data: {
                default_branch: 'main',
              },
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
        branch: 'feature-branch',
        path: '',
      });

      expect(result).toHaveProperty('error');
      expect(mockOctokit.rest.repos.getContent).toHaveBeenCalledTimes(1);
    });

    it('should not try common branches when requested branch fails', async () => {
      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi
              .fn()
              .mockRejectedValueOnce(createRequestError('Not Found', 404)),
            get: vi.fn().mockResolvedValue({
              data: {
                default_branch: 'main',
              },
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
        branch: 'nonexistent',
        path: '',
      });

      expect(result).toHaveProperty('error');
      expect(mockOctokit.rest.repos.getContent).toHaveBeenCalledTimes(1);
    });
  });
});
