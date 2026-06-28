import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

const executeDirectTool = vi.fn();

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
    vi.spyOn(console, 'log').mockImplementation(() => undefined);
    vi.spyOn(console, 'error').mockImplementation(() => undefined);
  });

  afterEach(() => {
    vi.restoreAllMocks();
    process.exitCode = undefined;
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
    expect(parsed.source).toBe('tree');
    expect(parsed.repoRoot).toBe('/tmp/octocode/tmp/tree/facebook/react/main');
    expect(parsed.localPath).toBe(
      '/tmp/octocode/tmp/tree/facebook/react/main/packages/react/index.js'
    );
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
    expect(parsed.localPath).toBe(
      '/tmp/octocode/tmp/tree/facebook/react/main/packages/react'
    );
    expect(parsed.repoRoot).toBe('/tmp/octocode/tmp/tree/facebook/react/main');
    expect(parsed.complete).toBe(true);
    expect(parsed.verified).toBe(true);
    expect(parsed.commitSha).toBe('0123456789abcdef0123456789abcdef01234567');
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

  // Regression: fetching a directory at the default `file` depth used to surface
  // the raw tool error "Use ghViewRepoStructure" — which lists, but doesn't bring
  // anything to disk. Point at the cache command's own subtree mode (and clone).
  it('rewrites the directory error to suggest --depth tree / clone', async () => {
    executeDirectTool.mockResolvedValue({
      isError: true,
      content: [
        {
          type: 'text',
          text: 'Path is a directory. Use ghViewRepoStructure to list directory contents',
        },
      ],
      structuredContent: {},
    });

    await run(['fetch', 'facebook/react', 'packages/react']);

    const errOut = vi.mocked(console.error).mock.calls.flat().join(' ');
    expect(errOut).toMatch(/--depth tree/);
    expect(errOut).toMatch(/clone facebook\/react\/packages\/react/);
    expect(errOut).not.toMatch(/ghViewRepoStructure/);
  });

  it('rewrites the directory error in --json mode too', async () => {
    executeDirectTool.mockResolvedValue({
      isError: true,
      content: [
        {
          type: 'text',
          text: 'Path is a directory. Use ghViewRepoStructure to list directory contents',
        },
      ],
      structuredContent: {},
    });

    await run(['fetch', 'facebook/react', 'packages/react'], { json: true });

    const output = vi.mocked(console.log).mock.calls.flat().join('\n');
    const parsed = JSON.parse(output) as { success: boolean; error: string };
    expect(parsed.success).toBe(false);
    expect(parsed.error).toMatch(/--depth tree/);
  });
});
