import { beforeEach, describe, expect, it, vi } from 'vitest';

const executeDirectTool = vi.fn();

vi.mock('@octocodeai/octocode-tools-core/direct', () => ({
  executeDirectTool: (...args: unknown[]) => executeDirectTool(...args),
}));

const { materializeRemoteForCli } =
  await import('../../src/cli/remote-local/materialize.js');

describe('remote clone materialization', () => {
  beforeEach(() => {
    executeDirectTool.mockReset();
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
      branch: 'main',
      localPath: '/tmp/octocode/react',
      repoRoot: '/tmp/octocode/react',
      source: 'clone',
      cached: true,
      location: {
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
    expect(result).toMatchObject({
      requestedPath: 'packages/react',
      repoRoot: '/tmp/octocode/react',
      localPath: '/tmp/octocode/react/packages/react',
      cached: false,
    });
  });
});
