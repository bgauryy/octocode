import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

const executeDirectTool = vi.fn();

const { mockPaths, getDirectorySizeBytes, existsSync, rmSync } = vi.hoisted(
  () => ({
    mockPaths: {
      home: '/fake/octocode',
      tmp: '/fake/octocode/tmp',
      clone: '/fake/octocode/tmp/clone',
      tree: '/fake/octocode/tmp/tree',
      response: '/fake/octocode/tmp/response',
      repos: '/fake/octocode/tmp/clone',
    },
    getDirectorySizeBytes: vi.fn((_path: string) => 1024),
    existsSync: vi.fn(() => false),
    rmSync: vi.fn(),
  })
);

vi.mock('node:fs', () => ({
  existsSync,
  rmSync,
  statSync: () => ({ isFile: () => false }),
}));

vi.mock('@octocodeai/octocode-tools-core/paths', () => ({ paths: mockPaths }));
vi.mock('@octocodeai/octocode-tools-core/fs-utils', () => ({
  getDirectorySizeBytes,
  formatBytes: (bytes: number) => `${bytes} B`,
}));

vi.mock('@octocodeai/octocode-tools-core/direct', () => ({
  executeDirectTool: (...args: unknown[]) => executeDirectTool(...args),
}));

vi.mock('../../../src/utils/colors.js', () => ({
  c: (_color: string, s: string) => s,
  dim: (s: string) => s,
}));

import { cacheCommand } from '../../../src/cli/commands/cache.js';
import type { ParsedArgs } from '../../../src/cli/types.js';

function run(args: string[], options: Record<string, string | boolean> = {}) {
  const parsed: ParsedArgs = { command: 'cache', args, options };
  return cacheCommand.handler(parsed);
}

function fetchFileEnvelope(
  localPath = '/tmp/octocode/tmp/tree/facebook/react/main/packages/react/index.js'
) {
  return {
    isError: false,
    content: [],
    structuredContent: {
      results: [
        {
          id: 'facebook/react',
          data: {
            owner: 'facebook',
            repo: 'react',
            files: [
              {
                path: 'packages/react/index.js',
                content: 'export {};',
                localPath,
                repoRoot: '/tmp/octocode/tmp/tree/facebook/react/main',
                resolvedBranch: 'main',
                cached: true,
              },
            ],
          },
        },
      ],
    },
  };
}

function fetchDirectoryEnvelope(
  localPath = '/tmp/octocode/tmp/tree/facebook/react/main/packages/react'
) {
  return {
    isError: false,
    content: [],
    structuredContent: {
      results: [
        {
          id: 'facebook/react',
          data: {
            owner: 'facebook',
            repo: 'react',
            directories: [
              {
                path: 'packages/react',
                localPath,
                repoRoot: '/tmp/octocode/tmp/tree/facebook/react/main',
                fileCount: 2,
                totalSize: 1234,
                complete: true,
                verified: true,
                commitSha: '0123456789abcdef0123456789abcdef01234567',
                cached: true,
                resolvedBranch: 'main',
              },
            ],
          },
        },
      ],
    },
  };
}

describe('cache command', () => {
  beforeEach(() => {
    executeDirectTool.mockReset();
    executeDirectTool.mockResolvedValue(fetchFileEnvelope());
    process.exitCode = undefined;
    existsSync.mockReset().mockReturnValue(false);
    rmSync.mockReset();
    vi.spyOn(console, 'log').mockImplementation(() => undefined);
    vi.spyOn(console, 'error').mockImplementation(() => undefined);
  });

  afterEach(() => {
    vi.restoreAllMocks();
    process.exitCode = undefined;
  });

  it('cache status reports the shared response bucket and real tmp total', async () => {
    getDirectorySizeBytes.mockImplementation((path: string) =>
      path === mockPaths.tmp ? 4096 : 1024
    );

    await run(['status'], { json: true });

    const parsed = JSON.parse(
      String(vi.mocked(console.log).mock.calls.at(-1)?.[0])
    ) as {
      tmp: { sizeBytes: number };
      response: { path: string; sizeBytes: number };
    };
    expect(parsed.tmp.sizeBytes).toBe(4096);
    expect(parsed.response).toEqual({
      path: mockPaths.response,
      exists: false,
      sizeBytes: 1024,
      sizeFormatted: '1024 B',
    });
  });

  it('cache clear --all removes the complete tmp cache, including responses and lifecycle state', async () => {
    existsSync.mockReturnValue(true);

    await run(['clear'], { all: true, json: true });

    expect(rmSync).toHaveBeenCalledOnce();
    expect(rmSync).toHaveBeenCalledWith(mockPaths.tmp, {
      recursive: true,
      force: true,
    });
    expect(
      JSON.parse(String(vi.mocked(console.log).mock.calls.at(-1)?.[0]))
    ).toEqual({
      success: true,
      cleared: { tmp: mockPaths.tmp },
    });
  });

  it('cache clear can reset clone and tree materializations without deleting responses', async () => {
    existsSync.mockReturnValue(true);

    await run(['clear'], { clone: true, tree: true, json: true });

    expect(rmSync).toHaveBeenCalledTimes(2);
    expect(rmSync).toHaveBeenCalledWith(mockPaths.clone, {
      recursive: true,
      force: true,
    });
    expect(rmSync).toHaveBeenCalledWith(mockPaths.tree, {
      recursive: true,
      force: true,
    });
    expect(rmSync).not.toHaveBeenCalledWith(
      mockPaths.response,
      expect.anything()
    );
  });

  it('cache fetch materializes a remote path and returns structured location data', async () => {
    await run(['fetch', 'facebook/react', 'packages/react/index.js'], {
      depth: 'file',
      json: true,
    });

    expect(executeDirectTool).toHaveBeenCalledWith(
      'ghGetFileContent',
      expect.objectContaining({
        queries: [
          expect.objectContaining({
            owner: 'facebook',
            repo: 'react',
            path: 'packages/react/index.js',
            type: 'file',
            fullContent: true,
            minify: 'none',
          }),
        ],
      })
    );

    const output = vi.mocked(console.log).mock.calls.flat().join('\n');
    const parsed = JSON.parse(output) as {
      success: boolean;
      source: string;
      localPath: string;
      repoRoot: string;
      location: {
        kind: string;
        localPath: string;
        repoRoot?: string;
        requestedPath?: string;
        source?: string;
        cached?: boolean;
        complete?: boolean;
        resolvedBranch?: string;
      };
    };
    expect(parsed.success).toBe(true);
    expect(parsed.location.source).toBe('tree');
    expect(parsed.location.repoRoot).toBe(
      '/tmp/octocode/tmp/tree/facebook/react/main'
    );
    expect(parsed.location.localPath).toBe(
      '/tmp/octocode/tmp/tree/facebook/react/main/packages/react/index.js'
    );
    expect(parsed).not.toHaveProperty('localPath');
    expect(parsed).not.toHaveProperty('repoRoot');
    expect(parsed.location.kind).toBe('file');
    expect(parsed.location.source).toBe('tree');
    expect(parsed.location.localPath).toBe(
      '/tmp/octocode/tmp/tree/facebook/react/main/packages/react/index.js'
    );
    expect(parsed.location.repoRoot).toBe(
      '/tmp/octocode/tmp/tree/facebook/react/main'
    );
    expect(parsed.location.requestedPath).toBe('packages/react/index.js');
    expect(parsed.location.resolvedBranch).toBe('main');
    expect(parsed.location.cached).toBe(true);
    expect(parsed.location.complete).toBe(true);
  });

  it('cache fetch --depth tree reads canonical directory rows', async () => {
    executeDirectTool.mockResolvedValue(fetchDirectoryEnvelope());

    await run(['fetch', 'facebook/react', 'packages/react'], {
      depth: 'tree',
      json: true,
    });

    expect(executeDirectTool).toHaveBeenCalledWith(
      'ghGetFileContent',
      expect.objectContaining({
        queries: [
          expect.objectContaining({
            owner: 'facebook',
            repo: 'react',
            path: 'packages/react',
            type: 'directory',
          }),
        ],
      })
    );

    const output = vi.mocked(console.log).mock.calls.flat().join('\n');
    const parsed = JSON.parse(output) as {
      success: boolean;
      localPath: string;
      repoRoot: string;
      complete: boolean;
      verified: boolean;
      commitSha?: string;
      location: {
        kind: string;
        localPath: string;
        repoRoot?: string;
        source?: string;
        complete?: boolean;
        verified?: boolean;
        commitSha?: string;
      };
    };
    expect(parsed.success).toBe(true);
    expect(parsed.location.localPath).toBe(
      '/tmp/octocode/tmp/tree/facebook/react/main/packages/react'
    );
    expect(parsed.location.repoRoot).toBe(
      '/tmp/octocode/tmp/tree/facebook/react/main'
    );
    expect(parsed.location.complete).toBe(true);
    expect(parsed.location.verified).toBe(true);
    expect(parsed.location.commitSha).toBe(
      '0123456789abcdef0123456789abcdef01234567'
    );
    expect(parsed.location.kind).toBe('directory');
    expect(parsed.location.source).toBe('tree');
    expect(parsed.location.localPath).toBe(
      '/tmp/octocode/tmp/tree/facebook/react/main/packages/react'
    );
    expect(parsed.location.repoRoot).toBe(
      '/tmp/octocode/tmp/tree/facebook/react/main'
    );
    expect(parsed.location.complete).toBe(true);
    expect(parsed.location.verified).toBe(true);
  });

  it.each(['LICENSE', '.github', 'src/a.ts'])(
    'uses clone as the unambiguous default for %s',
    async remotePath => {
      executeDirectTool.mockResolvedValue({
        structuredContent: {
          results: [
            {
              data: {
                location: {
                  localPath: '/tmp/repo',
                  complete: true,
                  cached: false,
                },
              },
            },
          ],
        },
      });
      await run(['fetch', 'facebook/react', remotePath], { json: true });
      expect(executeDirectTool).toHaveBeenCalledWith(
        'ghCloneRepo',
        expect.objectContaining({
          queries: [expect.objectContaining({ sparsePath: remotePath })],
        })
      );
    }
  );

  // Regression: fetching a directory at the default `file` depth used to surface
  // the raw tool error "Use github.tree" — which lists, but doesn't bring
  // anything to disk. Point at the cache command's own subtree mode (and clone).
  it('rewrites the directory error to suggest --depth tree / clone', async () => {
    executeDirectTool.mockResolvedValue({
      isError: true,
      content: [
        {
          type: 'text',
          text: 'Path is a directory. Use github.tree to list directory contents',
        },
      ],
      structuredContent: {},
    });

    await run(['fetch', 'facebook/react', 'packages/react'], { depth: 'file' });

    const errOut = vi.mocked(console.error).mock.calls.flat().join(' ');
    expect(errOut).toMatch(/--depth tree/);
    expect(errOut).toMatch(/--depth clone/);
    expect(errOut).not.toMatch(/github.tree/);
  });

  it('rewrites the directory error in --json mode too', async () => {
    executeDirectTool.mockResolvedValue({
      isError: true,
      content: [
        {
          type: 'text',
          text: 'Path is a directory. Use github.tree to list directory contents',
        },
      ],
      structuredContent: {},
    });

    await run(['fetch', 'facebook/react', 'packages/react'], {
      json: true,
      depth: 'file',
    });

    const output = vi.mocked(console.log).mock.calls.flat().join('\n');
    const parsed = JSON.parse(output) as { success: boolean; error: string };
    expect(parsed.success).toBe(false);
    expect(parsed.error).toMatch(/--depth tree/);
  });
});
