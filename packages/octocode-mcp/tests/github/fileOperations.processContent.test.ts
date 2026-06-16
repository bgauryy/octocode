import { describe, it, expect, beforeEach, vi } from 'vitest';
import { fetchGitHubFileContentAPI } from '../../../octocode-tools-core/src/github/fileContent.js';
import { viewGitHubRepositoryStructureAPI } from '../../../octocode-tools-core/src/github/repoStructure.js';
import {
  getOctokit,
  resolveDefaultBranch,
} from '../../../octocode-tools-core/src/github/client.js';
import { RequestError } from 'octokit';
import * as minifierModule from '@octocodeai/octocode-context-utils';
import {
  extractSignatures,
  applyContentViewMinification,
} from '@octocodeai/octocode-context-utils';
import { SIGNATURE_SOURCE } from '../fixtures/signatureSource.js';
import { clearAllCache } from '../../../octocode-tools-core/src/utils/http/cache.js';

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

vi.mock('../../../octocode-tools-core/src/github/client.js');
vi.mock('@octocodeai/octocode-context-utils', async importOriginal => {
  const actual =
    await importOriginal<typeof import('@octocodeai/octocode-context-utils')>();
  return { ...actual, minifyContent: vi.fn(), minifyContentSync: vi.fn() };
});

describe('GitHub File Operations - processFileContentAPI coverage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    clearAllCache();
    vi.mocked(resolveDefaultBranch).mockResolvedValue('main');
  });

  describe('fetchGitHubFileContentAPI - File Size and Encoding', () => {
    it('should decode files larger than 300KB that have inline content', async () => {
      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi.fn().mockResolvedValue({
              data: {
                type: 'file',
                content: Buffer.from('large file content').toString('base64'),
                size: 400 * 1024,
                sha: 'abc123',
                name: 'large-file.txt',
                path: 'large-file.txt',
              },
            }),
            listCommits: vi.fn().mockResolvedValue({ data: [] }),
          },
        },
      };

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as Awaited<ReturnType<typeof getOctokit>>
      );
      vi.mocked(minifierModule.minifyContent).mockResolvedValue({
        content: 'large file content',
        failed: false,
        type: 'general',
      });

      const result = await fetchGitHubFileContentAPI({
        owner: 'test',
        repo: 'repo',
        path: 'large-file.txt',
      });

      expect('error' in result).toBe(false);
      expect(result).toHaveProperty('data');
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
        mockOctokit as unknown as Awaited<ReturnType<typeof getOctokit>>
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

    it('minify:"symbols" returns the extracted skeleton, aligned with the local path', async () => {
      const SOURCE = SIGNATURE_SOURCE;

      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi.fn().mockResolvedValue({
              data: {
                type: 'file',
                content: Buffer.from(SOURCE).toString('base64'),
                size: SOURCE.length,
                sha: 'sig123',
                name: 'sample.ts',
                path: 'sample.ts',
              },
            }),
            listCommits: vi.fn().mockResolvedValue({ data: [] }),
          },
        },
      };
      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as Awaited<ReturnType<typeof getOctokit>>
      );

      const result = (await fetchGitHubFileContentAPI({
        owner: 'test',
        repo: 'repo',
        path: 'sample.ts',
        minify: 'symbols',
      } as unknown as Parameters<typeof fetchGitHubFileContentAPI>[0])) as {
        data: { content: string; sourceChars?: number; sourceBytes?: number };
      };

      expect('error' in result).toBe(false);
      const content = result.data.content;
      const rawSigs = extractSignatures(SOURCE, 'sample.ts')!;
      expect(content).toBe(applyContentViewMinification(rawSigs, 'sample.ts'));
      expect(content).toContain('interface Foo');
      expect(content).toContain('id: string;');
      expect(content).toContain('a: string,');
      expect(content).toContain('Promise<void>');
      expect(content).not.toContain('secretLocal');
      expect(result.data.sourceChars).toBe(SOURCE.length);
      expect(result.data.sourceBytes).toBe(Buffer.byteLength(SOURCE, 'utf-8'));
    });

    it('returns a large minify:"symbols" skeleton WHOLE — never paginated', async () => {
      let src = '';
      for (let i = 0; i < 400; i++) {
        src += `export function fn${i}(argOne: string, argTwo: number): Promise<void> {\n  return doStuff(${i});\n}\n`;
      }
      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi.fn().mockResolvedValue({
              data: {
                type: 'file',
                content: Buffer.from(src).toString('base64'),
                size: src.length,
                sha: 'big123',
                name: 'big.ts',
                path: 'big.ts',
              },
            }),
            listCommits: vi.fn().mockResolvedValue({ data: [] }),
          },
        },
      };
      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as Awaited<ReturnType<typeof getOctokit>>
      );

      const result = (await fetchGitHubFileContentAPI({
        owner: 'test',
        repo: 'repo',
        path: 'big.ts',
        minify: 'symbols',
        charOffset: 3000,
        charLength: 100,
      } as unknown as Parameters<typeof fetchGitHubFileContentAPI>[0])) as {
        data: {
          content: string;
          pagination?: { hasMore: boolean };
          isPartial?: boolean;
          sourceChars?: number;
          signaturesExtracted?: boolean;
        };
      };

      expect('error' in result).toBe(false);
      expect(result.data.pagination).toBeUndefined();
      expect(result.data.content).toContain('fn0(');
      expect(result.data.content).toContain('fn399(');
      expect(result.data.content).not.toContain('doStuff');
      expect(result.data.isPartial).toBe(false);
      expect(result.data.sourceChars).toBe(src.length);
      expect(result.data.signaturesExtracted).toBeUndefined();
    });

    it('redacts secrets inside minify:"symbols" output (aligned with local)', async () => {
      const src =
        'export function connect(token = "AKIAIOSFODNN7EXAMPLE"): void {\n  doThing();\n}\n';
      const mockOctokit = {
        rest: {
          repos: {
            getContent: vi.fn().mockResolvedValue({
              data: {
                type: 'file',
                content: Buffer.from(src).toString('base64'),
                size: src.length,
                sha: 'sec123',
                name: 'svc.ts',
                path: 'svc.ts',
              },
            }),
            listCommits: vi.fn().mockResolvedValue({ data: [] }),
          },
        },
      };
      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as Awaited<ReturnType<typeof getOctokit>>
      );

      const result = (await fetchGitHubFileContentAPI({
        owner: 'test',
        repo: 'repo',
        path: 'svc.ts',
        minify: 'symbols',
      } as unknown as Parameters<typeof fetchGitHubFileContentAPI>[0])) as {
        data: { content: string };
      };

      expect(result.data.content).toContain('connect(');
      expect(result.data.content).toContain('[REDACTED');
      expect(result.data.content).not.toContain('AKIAIOSFODNN7EXAMPLE');
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
        mockOctokit as unknown as Awaited<ReturnType<typeof getOctokit>>
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
        mockOctokit as unknown as Awaited<ReturnType<typeof getOctokit>>
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
        mockOctokit as unknown as Awaited<ReturnType<typeof getOctokit>>
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
        mockOctokit as unknown as Awaited<ReturnType<typeof getOctokit>>
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
        mockOctokit as unknown as Awaited<ReturnType<typeof getOctokit>>
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
        mockOctokit as unknown as Awaited<ReturnType<typeof getOctokit>>
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
        mockOctokit as unknown as Awaited<ReturnType<typeof getOctokit>>
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
        path: 'test.txt',
        matchString: 'TARGET LINE',
        contextLines: 1,
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
        mockOctokit as unknown as Awaited<ReturnType<typeof getOctokit>>
      );
      vi.mocked(minifierModule.minifyContent).mockImplementation(
        async content => ({ content, failed: false, type: 'general' })
      );

      const result = await fetchGitHubFileContentAPI({
        owner: 'test',
        repo: 'repo',
        path: 'test.txt',
        matchString: 'attachPingListener(root,wakeable,rootRenderLanes)',
        contextLines: 0,
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
        path: 'test.txt',
        matchString: 'Target Line',
        contextLines: 1,
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
        mockOctokit as unknown as Awaited<ReturnType<typeof getOctokit>>
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

  describe('fetchGitHubFileContentAPI - JSON content is minified by applyContentViewMinification', () => {
    it('minifies JSON content (sync inline minification, not async minifyContent)', async () => {
      const fileContent = '{\n  "name": "demo",\n  "version": "1.0.0"\n}';
      const minifiedJson = fileContent;

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
        mockOctokit as unknown as Awaited<ReturnType<typeof getOctokit>>
      );
      const asyncMinifySpy = vi
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
        minify: 'standard',
      } as Parameters<typeof fetchGitHubFileContentAPI>[0]);

      expect('data' in result).toBe(true);
      if ('data' in result && result.data) {
        expect(result.data.content).toBe(minifiedJson);
      }
      expect(asyncMinifySpy).not.toHaveBeenCalled();
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
        mockOctokit as unknown as Awaited<ReturnType<typeof getOctokit>>
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
        mockOctokit as unknown as Awaited<ReturnType<typeof getOctokit>>
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
