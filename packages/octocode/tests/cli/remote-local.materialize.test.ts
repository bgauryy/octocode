import { beforeEach, describe, expect, it, vi } from 'vitest';

const executeDirectTool = vi.fn();
const { statSync } = vi.hoisted(() => ({
  statSync: vi.fn(() => ({ isFile: (): boolean => false })),
}));
vi.mock('node:fs', () => ({ statSync, existsSync: () => false }));

vi.mock('@octocodeai/octocode-tools-core/direct', () => ({
  executeDirectTool: (...args: unknown[]) => executeDirectTool(...args),
}));

const { materializeRemoteForCli } =
  await import('../../src/cli/remote-local/materialize.js');

describe('remote clone materialization', () => {
  beforeEach(() => {
    executeDirectTool.mockReset();
    statSync.mockReset().mockReturnValue({ isFile: () => false });
  });

  it('reads the canonical ghCloneRepo data.location envelope', async () => {
    executeDirectTool.mockResolvedValue({
      isError: false,
      structuredContent: {
        results: [
          {
            index: 0,
            data: {
              owner: 'facebook',
              repo: 'react',
              location: {
                kind: 'repo',
                localPath: '/tmp/octocode/react',
                source: 'clone',
                cached: true,
                complete: true,
                commitSha: 'a'.repeat(40),
                resolvedBranch: 'main',
              },
            },
          },
        ],
      },
    });

    const result = await materializeRemoteForCli({
      repoRef: 'facebook/react',
      kind: 'repo',
    });

    expect(result).toMatchObject({
      owner: 'facebook',
      repo: 'react',
      location: {
        repoRoot: '/tmp/octocode/react',
        source: 'clone',
        cached: true,
        complete: true,
        commitSha: 'a'.repeat(40),
        verified: false,
        localPath: '/tmp/octocode/react',
        resolvedBranch: 'main',
      },
    });
  });

  it('resolves a sparse subtree beneath the canonical clone location', async () => {
    executeDirectTool.mockResolvedValue({
      isError: false,
      structuredContent: {
        results: [
          {
            index: 0,
            data: {
              location: {
                kind: 'tree',
                localPath: '/tmp/octocode/react',
                requestedPath: 'packages/react',
                source: 'clone',
                cached: false,
                complete: false,
                resolvedBranch: 'main',
              },
            },
          },
        ],
      },
    });

    const result = await materializeRemoteForCli({
      repoRef: 'facebook/react/packages/react',
      kind: 'repo',
    });

    expect(executeDirectTool).toHaveBeenCalledWith(
      'ghCloneRepo',
      expect.objectContaining({
        queries: [expect.objectContaining({ sparsePath: 'packages/react' })],
      })
    );
    expect(result.location).toMatchObject({
      requestedPath: 'packages/react',
      repoRoot: '/tmp/octocode/react',
      localPath: '/tmp/octocode/react/packages/react',
      cached: false,
      complete: false,
      verified: false,
    });
  });

  it('preserves directory recovery instead of upgrading an unknown result to complete', async () => {
    const next = {
      escalateToClone: {
        tool: 'ghCloneRepo',
        query: { owner: 'o', repo: 'r', sparsePath: 'src' },
      },
    };
    executeDirectTool.mockResolvedValue({
      structuredContent: {
        results: [
          {
            data: {
              directories: [
                {
                  localPath: '/tmp/r/src',
                  repoRoot: '/tmp/r',
                  isPartial: true,
                  partialReasons: ['fetchFailed'],
                  next,
                },
              ],
            },
          },
        ],
      },
    });
    const result = await materializeRemoteForCli({
      repoRef: 'o/r',
      path: 'src',
      kind: 'tree',
    });
    expect(result).toMatchObject({
      location: { complete: false, verified: false },
      isPartial: true,
      partialReasons: ['fetchFailed'],
      next,
    });
  });

  it('rejects the removed flat fetch envelope', async () => {
    executeDirectTool.mockResolvedValue({
      structuredContent: {
        results: [{ files: [{ localPath: '/tmp/legacy' }] }],
      },
    });
    await expect(
      materializeRemoteForCli({ repoRef: 'o/r', path: 'LICENSE', kind: 'file' })
    ).rejects.toThrow('localPath');
  });

  it('identifies a sparse extensionless file from the actual checkout', async () => {
    statSync.mockReturnValue({ isFile: () => true });
    executeDirectTool.mockResolvedValue({
      structuredContent: {
        results: [
          { data: { location: { localPath: '/tmp/repo', complete: true } } },
        ],
      },
    });
    const result = await materializeRemoteForCli({
      repoRef: 'o/r',
      path: 'LICENSE',
      kind: 'repo',
    });
    expect(result.location).toMatchObject({
      kind: 'file',
      localPath: '/tmp/repo/LICENSE',
    });
  });
});
