/**
 * Tests for pythonPackage.ts - specifically for uncovered branches
 */
import {
  describe,
  it,
  expect,
  vi,
  beforeEach,
  beforeAll,
  afterAll,
} from 'vitest';
import { clearAllCache } from '../../src/utils/http/cache.js';

let originalFetch: typeof fetch;

beforeAll(() => {
  originalFetch = globalThis.fetch;
  globalThis.fetch = vi.fn() as typeof fetch;
});

afterAll(() => {
  globalThis.fetch = originalFetch;
});

// Import after fetch is stubbed
import { searchPythonPackage } from '../../src/utils/package/python.js';

/** PyPI JSON body from legacy axios-shaped test payloads `{ data: ... }` */
function pypiOk(axiosShape: { data: Record<string, unknown> }): Response {
  return new Response(JSON.stringify(axiosShape.data), {
    status: 200,
    headers: { 'Content-Type': 'application/json' },
  });
}

describe('pythonPackage - branch coverage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    clearAllCache(); // Clear cache to ensure test isolation
    vi.mocked(fetch).mockReset();
  });

  describe('lastPublished extraction from releases', () => {

    it('should report rawResponseChars from the successful PyPI response body', async () => {
      const payload = {
        info: {
          name: 'test-pkg',
          version: '1.0.0',
          summary: 'Test package',
          keywords: '',
          project_urls: {},
        },
      };
      vi.mocked(fetch).mockResolvedValue(
        new Response(JSON.stringify(payload), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        })
      );

      const result = await searchPythonPackage('test-pkg', false);

      expect('packages' in result).toBe(true);
      if ('packages' in result) {
        expect(result.rawResponseChars).toBe(JSON.stringify(payload).length);
      }
    });

    it('should extract lastPublished from releases when available', async () => {
      vi.mocked(fetch).mockResolvedValue(
        pypiOk({
          data: {
            info: {
              name: 'test-pkg',
              version: '1.0.0',
              summary: 'Test package',
              keywords: '',
              project_urls: {},
            },
            releases: {
              '1.0.0': [
                {
                  upload_time: '2024-01-15T10:30:00',
                },
              ],
            },
          },
        })
      );

      const result = await searchPythonPackage('test-pkg', true);

      expect('packages' in result).toBe(true);
      if ('packages' in result) {
        const pkg = result.packages[0] as any;
        expect(pkg.lastPublished).toBe('2024-01-15T10:30:00');
      }
    });

    it('should handle releases with empty version array', async () => {
      vi.mocked(fetch).mockResolvedValue(
        pypiOk({
          data: {
            info: {
              name: 'test-pkg',
              version: '1.0.0',
              summary: 'Test package',
              keywords: '',
              project_urls: {},
            },
            releases: {
              '1.0.0': [], // Empty array
            },
          },
        })
      );

      const result = await searchPythonPackage('test-pkg', true);

      expect('packages' in result).toBe(true);
      if ('packages' in result) {
        const pkg = result.packages[0] as any;
        expect(pkg.lastPublished).toBeUndefined();
      }
    });

    it('should handle releases without upload_time', async () => {
      vi.mocked(fetch).mockResolvedValue(
        pypiOk({
          data: {
            info: {
              name: 'test-pkg',
              version: '1.0.0',
              summary: 'Test package',
              keywords: '',
              project_urls: {},
            },
            releases: {
              '1.0.0': [
                {
                  // No upload_time
                  filename: 'test-pkg-1.0.0.tar.gz',
                },
              ],
            },
          },
        })
      );

      const result = await searchPythonPackage('test-pkg', true);

      expect('packages' in result).toBe(true);
      if ('packages' in result) {
        const pkg = result.packages[0] as any;
        expect(pkg.lastPublished).toBeUndefined();
      }
    });

    it('should handle missing version in releases', async () => {
      vi.mocked(fetch).mockResolvedValue(
        pypiOk({
          data: {
            info: {
              name: 'test-pkg',
              version: '1.0.0',
              summary: 'Test package',
              keywords: '',
              project_urls: {},
            },
            releases: {
              '2.0.0': [
                {
                  upload_time: '2024-01-15T10:30:00',
                },
              ],
              // '1.0.0' not present
            },
          },
        })
      );

      const result = await searchPythonPackage('test-pkg', true);

      expect('packages' in result).toBe(true);
      if ('packages' in result) {
        const pkg = result.packages[0] as any;
        expect(pkg.lastPublished).toBeUndefined();
      }
    });

    it('should handle releases being null', async () => {
      vi.mocked(fetch).mockResolvedValue(
        pypiOk({
          data: {
            info: {
              name: 'test-pkg',
              version: '1.0.0',
              summary: 'Test package',
              keywords: '',
              project_urls: {},
            },
            releases: null,
          },
        })
      );

      const result = await searchPythonPackage('test-pkg', true);

      expect('packages' in result).toBe(true);
      if ('packages' in result) {
        const pkg = result.packages[0] as any;
        expect(pkg.lastPublished).toBeUndefined();
      }
    });
  });

  describe('HTTP status and errors', () => {
    it('should reject non-404 HTTP error responses', async () => {
      vi.mocked(fetch).mockResolvedValue(
        new Response('', { status: 500, statusText: 'Internal Server Error' })
      );

      await expect(searchPythonPackage('test-pkg', false)).rejects.toThrow(
        'PyPI returned 500'
      );
    });

    it('should handle 404 via name variations', async () => {
      vi.mocked(fetch)
        .mockResolvedValueOnce(new Response('', { status: 404 }))
        .mockResolvedValueOnce(
          new Response(
            JSON.stringify({
              info: {
                name: 'test_pkg',
                version: '1.0.0',
                summary: 'Found with underscore',
                keywords: '',
                project_urls: {},
              },
            }),
            { status: 200, headers: { 'Content-Type': 'application/json' } }
          )
        );

      const result = await searchPythonPackage('test-pkg', false);

      expect('packages' in result).toBe(true);
      if ('packages' in result) {
        expect(result.packages.length).toBe(1);
      }
    });
  });

  describe('project_urls key variations', () => {
    it('should find repo from "github" key in project_urls', async () => {
      vi.mocked(fetch).mockResolvedValue(
        pypiOk({
          data: {
            info: {
              name: 'test-pkg',
              version: '1.0.0',
              summary: 'Test',
              keywords: '',
              project_urls: {
                GitHub: 'https://github.com/test/repo',
              },
            },
          },
        })
      );

      const result = await searchPythonPackage('test-pkg', false);

      expect('packages' in result).toBe(true);
      if ('packages' in result) {
        expect((result.packages[0] as any)?.repository).toBe(
          'https://github.com/test/repo'
        );
      }
    });

    it('should find repo from "source code" key in project_urls', async () => {
      vi.mocked(fetch).mockResolvedValue(
        pypiOk({
          data: {
            info: {
              name: 'test-pkg',
              version: '1.0.0',
              summary: 'Test',
              keywords: '',
              project_urls: {
                'Source Code': 'https://github.com/test/repo',
              },
            },
          },
        })
      );

      const result = await searchPythonPackage('test-pkg', false);

      expect('packages' in result).toBe(true);
      if ('packages' in result) {
        expect((result.packages[0] as any)?.repository).toBe(
          'https://github.com/test/repo'
        );
      }
    });

    it('should skip non-github/gitlab/bitbucket URLs in project_urls', async () => {
      vi.mocked(fetch).mockResolvedValue(
        pypiOk({
          data: {
            info: {
              name: 'test-pkg',
              version: '1.0.0',
              summary: 'Test',
              keywords: '',
              project_urls: {
                Source: 'https://example.com/repo', // Not a known repo host
                Repository: 'https://myhost.com/repo', // Not a known repo host
              },
            },
          },
        })
      );

      const result = await searchPythonPackage('test-pkg', false);

      expect('packages' in result).toBe(true);
      if ('packages' in result) {
        expect((result.packages[0] as any)?.repository).toBeNull();
      }
    });
  });
});
