import { describe, expect, it, vi } from 'vitest';
import { fetchStructureViaGitTree } from '../../src/github/repoStructureTree.js';

describe('scoped recursive trees upstream budget', () => {
  it('returns the same scoped files while fetching only the subtree in two calls', async () => {
    const unrelated = Array.from({ length: 1000 }, (_, i) => ({
      path: `unrelated/${i}`,
      type: 'blob',
      sha: `sha-${i}`,
    }));
    const subtree = [{ path: 'index.ts', type: 'blob', sha: 'file', size: 42 }];
    const root = [
      ...unrelated,
      { path: 'packages/target', type: 'tree', sha: 'subtree' },
      { ...subtree[0], path: 'packages/target/index.ts' },
    ];
    let receivedNodes = 0;
    const getBranch = vi.fn(async () => ({
      data: { commit: { commit: { tree: { sha: 'root' } } } },
    }));
    const getContent = vi.fn(async () => {
      receivedNodes++;
      return {
        data: [{ type: 'dir', path: 'packages/target', sha: 'subtree' }],
      };
    });
    const getTree = vi.fn(async ({ tree_sha }: { tree_sha: string }) => {
      const tree = tree_sha === 'subtree' ? subtree : root;
      receivedNodes += tree.length;
      return {
        data: { tree, truncated: false },
        headers: { etag: 'subtree-etag' },
      };
    });
    const result = await fetchStructureViaGitTree(
      { rest: { repos: { getBranch, getContent }, git: { getTree } } } as never,
      {
        owner: 'owner',
        repo: 'repo',
        workingBranch: 'main',
        pathPrefix: 'packages/target',
        maxDepth: 3,
      }
    );
    expect(result.items).toMatchObject([
      { path: 'packages/target/index.ts', name: 'index.ts', size: 42 },
    ]);
    expect(result.items).toHaveLength(1);
    expect(receivedNodes).toBe(2);
    expect(
      getBranch.mock.calls.length +
        getContent.mock.calls.length +
        getTree.mock.calls.length
    ).toBe(2);
    expect(getContent).toHaveBeenCalledWith({
      owner: 'owner',
      repo: 'repo',
      path: 'packages',
      ref: 'main',
    });
    expect(getTree).toHaveBeenCalledWith(
      expect.objectContaining({ tree_sha: 'subtree', recursive: 'true' })
    );
  });

  it('rejects an unresolved scoped directory so orchestration can use Contents fallback', async () => {
    const getBranch = vi.fn(async () => ({
      data: { commit: { commit: { tree: { sha: 'root' } } } },
    }));
    const getContent = vi.fn(async () => ({ data: [] }));
    const getTree = vi.fn(async () => ({
      data: { tree: [], truncated: false },
      headers: {},
    }));
    await expect(
      fetchStructureViaGitTree(
        {
          rest: { repos: { getBranch, getContent }, git: { getTree } },
        } as never,
        {
          owner: 'owner',
          repo: 'repo',
          workingBranch: 'main',
          pathPrefix: 'missing',
          maxDepth: 3,
        }
      )
    ).rejects.toThrow('Could not resolve tree SHA');
    expect(getTree).not.toHaveBeenCalled();
  });
});
