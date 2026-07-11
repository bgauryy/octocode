import { beforeEach, describe, expect, it, vi } from 'vitest';

const getRepoStructure = vi.fn();
const resolveDefaultBranch = vi.fn();

const fakeProvider = {
  capabilities: {},
  getRepoStructure: (...args: unknown[]) => getRepoStructure(...args),
  resolveDefaultBranch: (...args: unknown[]) => resolveDefaultBranch(...args),
};

vi.mock('../../../src/providers/factory.js', () => ({
  getProvider: () => fakeProvider,
}));

import { exploreMultipleRepositoryStructures } from '../../../src/tools/github_view_repo_structure/execution.js';

function ok(branch: string) {
  return {
    data: {
      branch,
      path: '/',
      structure: { '.': { files: ['README.md'], folders: [] } },
      summary: { totalFiles: 1, totalFolders: 0 },
    },
    status: 200,
    provider: 'github',
  };
}

function notFound() {
  return { error: 'Not Found', status: 404, provider: 'github' };
}

describe('ghViewRepoStructure — explicit invalid branch falls back to default (regression)', () => {
  beforeEach(() => {
    getRepoStructure.mockReset();
    resolveDefaultBranch.mockReset();
  });

  it('retries against the default branch and reports branchFallback when an explicit branch 404s', async () => {
    resolveDefaultBranch.mockResolvedValue('main');
    getRepoStructure
      .mockResolvedValueOnce(notFound())
      .mockResolvedValueOnce(ok('main'));

    const result = await exploreMultipleRepositoryStructures({
      queries: [
        {
          owner: 'facebook',
          repo: 'react',
          branch: 'no-such-branch-zzz',
          path: '',
        },
      ],
    } as never);

    expect(getRepoStructure).toHaveBeenCalledTimes(2);
    const text = JSON.stringify(result.structuredContent ?? result);
    expect(text).toContain('branchFallback');
    expect(text).toContain('no-such-branch-zzz');
    expect(text).toContain('README.md');
  });

  it('a genuinely missing repo (both branches fail) still returns the original error, not a false fallback', async () => {
    resolveDefaultBranch.mockResolvedValue('main');
    getRepoStructure.mockResolvedValue(notFound());

    const result = await exploreMultipleRepositoryStructures({
      queries: [
        {
          owner: 'no',
          repo: 'such-repo',
          branch: 'no-such-branch-zzz',
          path: '',
        },
      ],
    } as never);

    const text = JSON.stringify(result.structuredContent ?? result);
    expect(text).not.toContain('branchFallback');
    expect(text.toLowerCase()).toContain('not found');
  });

  it('an omitted branch does not trigger the retry path (only one call)', async () => {
    resolveDefaultBranch.mockResolvedValue('main');
    getRepoStructure.mockResolvedValue(ok('main'));

    await exploreMultipleRepositoryStructures({
      queries: [{ owner: 'facebook', repo: 'react', path: '' }],
    } as never);

    expect(getRepoStructure).toHaveBeenCalledTimes(1);
  });
});
