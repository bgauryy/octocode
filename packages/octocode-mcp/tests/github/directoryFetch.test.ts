import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import {
  existsSync,
  readFileSync,
  writeFileSync,
  rmSync,
  mkdirSync,
} from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';

const mockGetOctokit = vi.hoisted(() =>
  vi.fn().mockResolvedValue({
    rest: {
      repos: {
        getContent: vi.fn(),
      },
    },
  })
);

vi.mock('../../../octocode-tools-core/src/github/client.js', () => ({
  getOctokit: mockGetOctokit,
}));

const mockGetOctocodeDir = vi.hoisted(() => vi.fn());
vi.mock('octocode-shared', async importOriginal => {
  const actual = await importOriginal<typeof import('octocode-shared')>();
  return {
    ...actual,
    getOctocodeDir: mockGetOctocodeDir,
  };
});

const mockFetch = vi.fn();
vi.stubGlobal('fetch', mockFetch);

import {
  fetchDirectoryContents,
  MAX_DIRECTORY_FILES,
  MAX_TOTAL_SIZE,
} from '../../../octocode-tools-core/src/github/directoryFetch.js';

let testDir: string;

function createOctocodeTestDir(): string {
  const dir = join(
    tmpdir(),
    `octocode-test-${Date.now()}-${Math.random().toString(36).slice(2)}`
  );
  mkdirSync(dir, { recursive: true });
  return dir;
}

function mockDirectoryListing(
  files: Array<{ name: string; path: string; size: number }>
) {
  const data = files.map(f => ({
    name: f.name,
    path: f.path,
    type: 'file',
    size: f.size,
    download_url: `https://raw.githubusercontent.com/owner/repo/main/${f.path}`,
    sha: 'abc123',
  }));
  mockGetOctokit.mockResolvedValue({
    rest: {
      repos: {
        getContent: vi.fn().mockResolvedValue({ data }),
      },
    },
  });
}

function mockFetchResponses(contents: Record<string, string>) {
  mockFetch.mockImplementation(async (url: string) => {
    for (const [path, content] of Object.entries(contents)) {
      if (url.includes(path)) {
        return {
          ok: true,
          text: async () => content,
        };
      }
    }
    return { ok: false, status: 404 };
  });
}

describe('directoryFetch', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    testDir = createOctocodeTestDir();
    mockGetOctocodeDir.mockReturnValue(testDir);
  });

  afterEach(() => {
    try {
      if (existsSync(testDir)) {
        rmSync(testDir, { recursive: true, force: true });
      }
    } catch {
      void 0;
    }
  });

  describe('fetchDirectoryContents', () => {
    it('should fetch directory files and save to disk', async () => {
      mockDirectoryListing([
        { name: 'index.ts', path: 'src/index.ts', size: 100 },
        { name: 'utils.ts', path: 'src/utils.ts', size: 200 },
      ]);
      mockFetchResponses({
        'src/index.ts': 'export const main = true;',
        'src/utils.ts': 'export function helper() {}',
      });

      const result = await fetchDirectoryContents(
        'owner',
        'repo',
        'src',
        'main'
      );

      expect(result.cached).toBe(false);
      expect(result.fileCount).toBe(2);
      expect(result.owner).toBe('owner');
      expect(result.repo).toBe('repo');
      expect(result.branch).toBe('main');
      expect(result.directoryPath).toBe('src');
      expect(result.localPath).toContain('src');

      const cloneDir = join(testDir, 'repos', 'owner', 'repo', 'main');
      expect(existsSync(join(cloneDir, 'src', 'index.ts'))).toBe(true);
      expect(existsSync(join(cloneDir, 'src', 'utils.ts'))).toBe(true);

      const content1 = readFileSync(join(cloneDir, 'src', 'index.ts'), 'utf-8');
      expect(content1).toBe('export const main = true;');

      const content2 = readFileSync(join(cloneDir, 'src', 'utils.ts'), 'utf-8');
      expect(content2).toBe('export function helper() {}');
    });

    it('should return cache hit if directory exists and cache is valid', async () => {
      mockDirectoryListing([
        { name: 'file.ts', path: 'src/file.ts', size: 50 },
      ]);
      mockFetchResponses({ 'src/file.ts': 'content' });

      const result1 = await fetchDirectoryContents(
        'owner',
        'repo',
        'src',
        'main'
      );
      expect(result1.cached).toBe(false);

      const result2 = await fetchDirectoryContents(
        'owner',
        'repo',
        'src',
        'main'
      );
      expect(result2.cached).toBe(true);
      expect(result2.localPath).toBe(result1.localPath);
    });

    it('should skip binary files by extension', async () => {
      mockDirectoryListing([
        { name: 'code.ts', path: 'src/code.ts', size: 100 },
        { name: 'image.png', path: 'src/image.png', size: 5000 },
        { name: 'archive.zip', path: 'src/archive.zip', size: 10000 },
        { name: 'style.css', path: 'src/style.css', size: 200 },
      ]);
      mockFetchResponses({
        'src/code.ts': 'const x = 1;',
        'src/style.css': 'body {}',
      });

      const result = await fetchDirectoryContents(
        'owner',
        'repo',
        'src',
        'main'
      );

      expect(result.fileCount).toBe(2);
      expect(result.files.map(f => f.path)).toEqual(
        expect.arrayContaining(['src/code.ts', 'src/style.css'])
      );
    });

    it('should skip files larger than MAX_FILE_SIZE', async () => {
      mockDirectoryListing([
        { name: 'small.ts', path: 'src/small.ts', size: 100 },
        { name: 'huge.ts', path: 'src/huge.ts', size: 400 * 1024 },
      ]);
      mockFetchResponses({
        'src/small.ts': 'small content',
      });

      const result = await fetchDirectoryContents(
        'owner',
        'repo',
        'src',
        'main'
      );

      expect(result.fileCount).toBe(1);
      expect(result.files[0]!.path).toBe('src/small.ts');
    });

    it('should throw when path is not a directory', async () => {
      mockGetOctokit.mockResolvedValue({
        rest: {
          repos: {
            getContent: vi.fn().mockResolvedValue({
              data: {
                name: 'file.ts',
                path: 'src/file.ts',
                type: 'file',
                content: 'Y29udGVudA==',
              },
            }),
          },
        },
      });

      await expect(
        fetchDirectoryContents('owner', 'repo', 'src/file.ts', 'main')
      ).rejects.toThrow('not a directory');
    });

    it('should limit files to MAX_DIRECTORY_FILES', async () => {
      const files = Array.from({ length: 80 }, (_, i) => ({
        name: `file${i}.ts`,
        path: `src/file${i}.ts`,
        size: 50,
      }));
      mockDirectoryListing(files);

      const fetchResponses: Record<string, string> = {};
      for (const f of files) {
        fetchResponses[f.path] = `content of ${f.name}`;
      }
      mockFetchResponses(fetchResponses);

      const result = await fetchDirectoryContents(
        'owner',
        'repo',
        'src',
        'main'
      );

      expect(result.fileCount).toBeLessThanOrEqual(MAX_DIRECTORY_FILES);
    });

    it('should enforce total size limit', async () => {
      const largeContent = 'x'.repeat(2 * 1024 * 1024);
      mockDirectoryListing([
        { name: 'a.ts', path: 'src/a.ts', size: 100 },
        { name: 'b.ts', path: 'src/b.ts', size: 100 },
        { name: 'c.ts', path: 'src/c.ts', size: 100 },
      ]);
      mockFetchResponses({
        'src/a.ts': largeContent,
        'src/b.ts': largeContent,
        'src/c.ts': largeContent,
      });

      const result = await fetchDirectoryContents(
        'owner',
        'repo',
        'src',
        'main'
      );

      expect(result.totalSize).toBeLessThanOrEqual(MAX_TOTAL_SIZE);
    });

    it('should skip directories in listing', async () => {
      mockGetOctokit.mockResolvedValue({
        rest: {
          repos: {
            getContent: vi.fn().mockResolvedValue({
              data: [
                {
                  name: 'subdir',
                  path: 'src/subdir',
                  type: 'dir',
                  size: 0,
                  download_url: null,
                },
                {
                  name: 'file.ts',
                  path: 'src/file.ts',
                  type: 'file',
                  size: 100,
                  download_url:
                    'https://raw.githubusercontent.com/owner/repo/main/src/file.ts',
                },
              ],
            }),
          },
        },
      });
      mockFetchResponses({ 'src/file.ts': 'content' });

      const result = await fetchDirectoryContents(
        'owner',
        'repo',
        'src',
        'main'
      );

      expect(result.fileCount).toBe(1);
      expect(result.files[0]!.path).toBe('src/file.ts');
    });

    it('should gracefully handle individual file fetch failures', async () => {
      mockDirectoryListing([
        { name: 'good.ts', path: 'src/good.ts', size: 100 },
        { name: 'bad.ts', path: 'src/bad.ts', size: 100 },
      ]);
      mockFetch.mockImplementation(async (url: string) => {
        if (url.includes('bad.ts')) {
          return { ok: false, status: 500 };
        }
        return {
          ok: true,
          text: async () => 'good content',
        };
      });

      const result = await fetchDirectoryContents(
        'owner',
        'repo',
        'src',
        'main'
      );

      expect(result.fileCount).toBe(1);
      expect(result.files[0]!.path).toBe('src/good.ts');
    });

    it('should write cache metadata after fetching', async () => {
      mockDirectoryListing([
        { name: 'file.ts', path: 'src/file.ts', size: 50 },
      ]);
      mockFetchResponses({ 'src/file.ts': 'content' });

      await fetchDirectoryContents('owner', 'repo', 'src', 'main');

      const metaPath = join(
        testDir,
        'repos',
        'owner',
        'repo',
        'main',
        '.octocode-clone-meta.json'
      );
      expect(existsSync(metaPath)).toBe(true);

      const meta = JSON.parse(readFileSync(metaPath, 'utf-8'));
      expect(meta.owner).toBe('owner');
      expect(meta.repo).toBe('repo');
      expect(meta.branch).toBe('main');
      expect(meta.expiresAt).toBeDefined();
    });

    it('should set expiresAt to 24 hours from now', async () => {
      mockDirectoryListing([
        { name: 'file.ts', path: 'src/file.ts', size: 50 },
      ]);
      mockFetchResponses({ 'src/file.ts': 'content' });

      const before = Date.now();
      const result = await fetchDirectoryContents(
        'owner',
        'repo',
        'src',
        'main'
      );
      const after = Date.now();

      const expiresAt = new Date(result.expiresAt).getTime();
      const twentyFourHours = 24 * 60 * 60 * 1000;

      expect(expiresAt).toBeGreaterThanOrEqual(before + twentyFourHours - 1000);
      expect(expiresAt).toBeLessThanOrEqual(after + twentyFourHours + 1000);
    });

    it('should use same cache path as clone tool', async () => {
      mockDirectoryListing([
        { name: 'file.ts', path: 'src/file.ts', size: 50 },
      ]);
      mockFetchResponses({ 'src/file.ts': 'content' });

      const result = await fetchDirectoryContents(
        'owner',
        'repo',
        'src',
        'main'
      );

      expect(result.localPath).toBe(
        join(testDir, 'repos', 'owner', 'repo', 'main', 'src')
      );
    });

    it('should write source: directoryFetch in cache metadata', async () => {
      mockDirectoryListing([
        { name: 'file.ts', path: 'src/file.ts', size: 50 },
      ]);
      mockFetchResponses({ 'src/file.ts': 'content' });

      await fetchDirectoryContents('owner', 'repo', 'src', 'main');

      const metaPath = join(
        testDir,
        'repos',
        'owner',
        'repo',
        'main',
        '.octocode-clone-meta.json'
      );
      const meta = JSON.parse(readFileSync(metaPath, 'utf-8'));
      expect(meta.source).toBe('directoryFetch');
    });

    it('should remove stale files when re-fetching an expired directory', async () => {
      const cloneDir = join(testDir, 'repos', 'owner', 'repo', 'main');
      const dirPath = join(cloneDir, 'src');

      mkdirSync(dirPath, { recursive: true });
      writeFileSync(join(dirPath, 'old_stale.ts'), 'stale content');
      writeFileSync(
        join(cloneDir, '.octocode-clone-meta.json'),
        JSON.stringify({
          clonedAt: new Date(Date.now() - 48 * 60 * 60 * 1000).toISOString(),
          expiresAt: new Date(Date.now() - 24 * 60 * 60 * 1000).toISOString(),
          owner: 'owner',
          repo: 'repo',
          branch: 'main',
          source: 'directoryFetch',
        })
      );

      expect(existsSync(join(dirPath, 'old_stale.ts'))).toBe(true);

      mockDirectoryListing([
        { name: 'new_file.ts', path: 'src/new_file.ts', size: 50 },
      ]);
      mockFetchResponses({ 'src/new_file.ts': 'fresh content' });

      const result = await fetchDirectoryContents(
        'owner',
        'repo',
        'src',
        'main'
      );

      expect(result.cached).toBe(false);
      expect(result.fileCount).toBe(1);

      expect(existsSync(join(dirPath, 'old_stale.ts'))).toBe(false);
      expect(existsSync(join(dirPath, 'new_file.ts'))).toBe(true);
      expect(readFileSync(join(dirPath, 'new_file.ts'), 'utf-8')).toBe(
        'fresh content'
      );
    });

    it('should bypass cache when forceRefresh is true', async () => {
      mockDirectoryListing([
        { name: 'file.ts', path: 'src/file.ts', size: 50 },
      ]);
      mockFetchResponses({ 'src/file.ts': 'version 1' });

      const result1 = await fetchDirectoryContents(
        'owner',
        'repo',
        'src',
        'main'
      );
      expect(result1.cached).toBe(false);

      mockDirectoryListing([
        { name: 'file.ts', path: 'src/file.ts', size: 60 },
      ]);
      mockFetchResponses({ 'src/file.ts': 'version 2' });

      const result2 = await fetchDirectoryContents(
        'owner',
        'repo',
        'src',
        'main',
        undefined,
        true
      );
      expect(result2.cached).toBe(false);

      const cloneDir = join(testDir, 'repos', 'owner', 'repo', 'main');
      expect(readFileSync(join(cloneDir, 'src', 'file.ts'), 'utf-8')).toBe(
        'version 2'
      );
    });

    it('should evict expired clones on cache miss', async () => {
      const expiredDir = join(
        testDir,
        'repos',
        'stale-owner',
        'stale-repo',
        'main'
      );
      mkdirSync(expiredDir, { recursive: true });
      writeFileSync(
        join(expiredDir, '.octocode-clone-meta.json'),
        JSON.stringify({
          clonedAt: new Date(Date.now() - 48 * 60 * 60 * 1000).toISOString(),
          expiresAt: new Date(Date.now() - 24 * 60 * 60 * 1000).toISOString(),
          owner: 'stale-owner',
          repo: 'stale-repo',
          branch: 'main',
          source: 'directoryFetch',
        })
      );

      expect(existsSync(expiredDir)).toBe(true);

      mockDirectoryListing([
        { name: 'file.ts', path: 'src/file.ts', size: 50 },
      ]);
      mockFetchResponses({ 'src/file.ts': 'content' });

      await fetchDirectoryContents('owner', 'repo', 'src', 'main');

      expect(existsSync(expiredDir)).toBe(false);
    });

    it('should use clone cache when source is "clone" (never degrade)', async () => {
      const cloneDir = join(testDir, 'repos', 'owner', 'repo', 'main');
      const dirPath = join(cloneDir, 'src');

      mkdirSync(dirPath, { recursive: true });
      writeFileSync(join(dirPath, 'index.ts'), 'clone content');
      writeFileSync(
        join(cloneDir, '.octocode-clone-meta.json'),
        JSON.stringify({
          clonedAt: new Date().toISOString(),
          expiresAt: new Date(Date.now() + 24 * 60 * 60 * 1000).toISOString(),
          owner: 'owner',
          repo: 'repo',
          branch: 'main',
          source: 'clone',
        })
      );

      const result = await fetchDirectoryContents(
        'owner',
        'repo',
        'src',
        'main'
      );

      expect(result.cached).toBe(true);
      expect(result.files.length).toBeGreaterThan(0);
      expect(readFileSync(join(dirPath, 'index.ts'), 'utf-8')).toBe(
        'clone content'
      );
      const meta = JSON.parse(
        readFileSync(join(cloneDir, '.octocode-clone-meta.json'), 'utf-8')
      );
      expect(meta.source).toBe('clone');
    });

    it('should use clone cache even when forceRefresh is true', async () => {
      const cloneDir = join(testDir, 'repos', 'owner', 'repo', 'main');
      const dirPath = join(cloneDir, 'src');

      mkdirSync(dirPath, { recursive: true });
      writeFileSync(join(dirPath, 'app.ts'), 'clone version');
      writeFileSync(
        join(cloneDir, '.octocode-clone-meta.json'),
        JSON.stringify({
          clonedAt: new Date().toISOString(),
          expiresAt: new Date(Date.now() + 24 * 60 * 60 * 1000).toISOString(),
          owner: 'owner',
          repo: 'repo',
          branch: 'main',
          source: 'clone',
        })
      );

      const result = await fetchDirectoryContents(
        'owner',
        'repo',
        'src',
        'main',
        undefined,
        true
      );

      expect(result.cached).toBe(true);
      expect(readFileSync(join(dirPath, 'app.ts'), 'utf-8')).toBe(
        'clone version'
      );
    });

    it('should refetch via API when meta omits source (invalid cache)', async () => {
      const cloneDir = join(testDir, 'repos', 'owner', 'repo', 'main');
      const dirPath = join(cloneDir, 'src');

      mkdirSync(dirPath, { recursive: true });
      writeFileSync(join(dirPath, 'legacy.ts'), 'old clone');
      writeFileSync(
        join(cloneDir, '.octocode-clone-meta.json'),
        JSON.stringify({
          clonedAt: new Date().toISOString(),
          expiresAt: new Date(Date.now() + 24 * 60 * 60 * 1000).toISOString(),
          owner: 'owner',
          repo: 'repo',
          branch: 'main',
        })
      );

      mockDirectoryListing([
        { name: 'fresh.ts', path: 'src/fresh.ts', size: 10 },
      ]);
      mockFetchResponses({ 'src/fresh.ts': 'from api' });

      const result = await fetchDirectoryContents(
        'owner',
        'repo',
        'src',
        'main'
      );

      expect(result.cached).toBe(false);
      expect(existsSync(join(dirPath, 'fresh.ts'))).toBe(true);
      expect(readFileSync(join(dirPath, 'fresh.ts'), 'utf-8')).toBe('from api');
    });

    it('should throw when path not found in clone cache', async () => {
      const cloneDir = join(testDir, 'repos', 'owner', 'repo', 'main');

      mkdirSync(cloneDir, { recursive: true });
      writeFileSync(
        join(cloneDir, '.octocode-clone-meta.json'),
        JSON.stringify({
          clonedAt: new Date().toISOString(),
          expiresAt: new Date(Date.now() + 24 * 60 * 60 * 1000).toISOString(),
          owner: 'owner',
          repo: 'repo',
          branch: 'main',
          source: 'clone',
        })
      );

      await expect(
        fetchDirectoryContents('owner', 'repo', 'nonexistent', 'main')
      ).rejects.toThrow('not found in the cloned repository');
    });

    it('should allow forceRefresh on directoryFetch cache', async () => {
      mockDirectoryListing([
        { name: 'file.ts', path: 'src/file.ts', size: 50 },
      ]);
      mockFetchResponses({ 'src/file.ts': 'version 1' });

      await fetchDirectoryContents('owner', 'repo', 'src', 'main');

      const cloneDir = join(testDir, 'repos', 'owner', 'repo', 'main');
      const meta = JSON.parse(
        readFileSync(join(cloneDir, '.octocode-clone-meta.json'), 'utf-8')
      );
      expect(meta.source).toBe('directoryFetch');

      mockDirectoryListing([
        { name: 'file.ts', path: 'src/file.ts', size: 60 },
      ]);
      mockFetchResponses({ 'src/file.ts': 'version 2' });

      const result = await fetchDirectoryContents(
        'owner',
        'repo',
        'src',
        'main',
        undefined,
        true
      );
      expect(result.cached).toBe(false);
      expect(readFileSync(join(cloneDir, 'src', 'file.ts'), 'utf-8')).toBe(
        'version 2'
      );
    });

    it('should override existing files with fresh content', async () => {
      mockDirectoryListing([{ name: 'app.ts', path: 'src/app.ts', size: 50 }]);
      mockFetchResponses({ 'src/app.ts': 'version 1' });

      const result1 = await fetchDirectoryContents(
        'owner',
        'repo',
        'src',
        'main'
      );
      expect(result1.cached).toBe(false);

      const cloneDir = join(testDir, 'repos', 'owner', 'repo', 'main');
      expect(readFileSync(join(cloneDir, 'src', 'app.ts'), 'utf-8')).toBe(
        'version 1'
      );

      writeFileSync(
        join(cloneDir, '.octocode-clone-meta.json'),
        JSON.stringify({
          clonedAt: new Date(Date.now() - 48 * 60 * 60 * 1000).toISOString(),
          expiresAt: new Date(Date.now() - 1000).toISOString(),
          owner: 'owner',
          repo: 'repo',
          branch: 'main',
          source: 'directoryFetch',
        })
      );

      mockDirectoryListing([{ name: 'app.ts', path: 'src/app.ts', size: 60 }]);
      mockFetchResponses({ 'src/app.ts': 'version 2' });

      const result2 = await fetchDirectoryContents(
        'owner',
        'repo',
        'src',
        'main'
      );
      expect(result2.cached).toBe(false);

      expect(readFileSync(join(cloneDir, 'src', 'app.ts'), 'utf-8')).toBe(
        'version 2'
      );
    });
  });
});
