import { describe, it, expect, vi, beforeEach } from 'vitest';
import { searchPackages } from '../../../octocode-tools-core/src/tools/package_search/execution.js';
import * as packageCommon from '../../../octocode-tools-core/src/utils/package/common.js';

vi.mock('../../../octocode-tools-core/src/utils/package/common.js', () => ({
  searchPackage: vi.fn(),
  checkNpmDeprecation: vi.fn().mockResolvedValue(null),
}));

const mockSearchPackage = vi.mocked(packageCommon.searchPackage);

const BASE = {
  mainResearchGoal: 'Test',
  researchGoal: 'Find package',
  reasoning: 'Unit test',
};

function pkg(overrides: Record<string, unknown> = {}) {
  return {
    name: 'mypkg',
    npmUrl: 'https://www.npmjs.com/package/mypkg',
    version: '1.0.0',
    repoUrl: 'https://github.com/owner/mypkg',
    mainEntry: null,
    typeDefinitions: null,
    ...overrides,
  };
}

function callTool(packageName: string, extra: Record<string, unknown> = {}) {
  return searchPackages({
    queries: [{ ...BASE, packageName, ...extra } as never],
  });
}

function text(result: Awaited<ReturnType<typeof searchPackages>>): string {
  return (result.content as { text?: string }[])?.[0]?.text ?? '';
}

describe('input validation', () => {
  beforeEach(() => vi.clearAllMocks());

  it('errors when packageName is missing', async () => {
    const r = await searchPackages({ queries: [{ ...BASE } as never] });
    expect(text(r).toLowerCase()).toContain('required');
    expect(mockSearchPackage).not.toHaveBeenCalled();
  });

  it('errors when packageName is empty string', async () => {
    const r = await callTool('');
    expect(r.isError).toBe(true);
    expect(mockSearchPackage).not.toHaveBeenCalled();
  });
});

describe('output format — object list with rich metadata', () => {
  beforeEach(() => vi.clearAllMocks());

  it('flat repo → object with name and repository fields', async () => {
    mockSearchPackage.mockResolvedValue({
      packages: [
        pkg({ name: 'zod', repoUrl: 'https://github.com/colinhacks/zod' }),
      ],
      totalFound: 1,
    });
    const t = text(await callTool('zod'));
    expect(t).toContain('name: zod');
    expect(t).toContain('repository: https://github.com/colinhacks/zod');
    expect(t).not.toContain('repositoryDirectory:'); // no sourceRoot for flat repo
  });

  it('monorepo → includes repositoryDirectory field', async () => {
    mockSearchPackage.mockResolvedValue({
      packages: [
        pkg({
          name: 'react',
          repoUrl: 'https://github.com/facebook/react',
          repositoryDirectory: 'packages/react',
        }),
      ],
      totalFound: 1,
    });
    const t = text(await callTool('react'));
    expect(t).toContain('name: react');
    expect(t).toContain('repository: https://github.com/facebook/react');
    expect(t).toContain('repositoryDirectory: packages/react');
  });

  it('strips leading "./" from repositoryDirectory', async () => {
    mockSearchPackage.mockResolvedValue({
      packages: [
        pkg({
          name: 'pkg-a',
          repoUrl: 'https://github.com/org/mono',
          repositoryDirectory: './packages/pkg-a',
        }),
      ],
      totalFound: 1,
    });
    const t = text(await callTool('pkg-a'));
    expect(t).toContain('repositoryDirectory: packages/pkg-a');
    expect(t).not.toContain('./packages');
  });

  it('non-GitHub repoUrl → included in repository field', async () => {
    mockSearchPackage.mockResolvedValue({
      packages: [pkg({ repoUrl: 'https://gitlab.com/owner/repo' })],
      totalFound: 1,
    });
    const t = text(await callTool('mypkg'));
    expect(t).toContain('name: mypkg');
    expect(t).toContain('repository: https://gitlab.com/owner/repo');
  });

  it('null repoUrl → no repository field', async () => {
    mockSearchPackage.mockResolvedValue({
      packages: [pkg({ repoUrl: null })],
      totalFound: 1,
    });
    const t = text(await callTool('mypkg'));
    expect(t).toContain('name: mypkg');
    expect(t).not.toContain('repository: https://');
  });

  it('multiple packages → one object entry each', async () => {
    mockSearchPackage.mockResolvedValue({
      packages: [
        pkg({ name: 'zustand', repoUrl: 'https://github.com/pmndrs/zustand' }),
        pkg({ name: 'jotai', repoUrl: 'https://github.com/pmndrs/jotai' }),
        pkg({
          name: '@tanstack/query',
          repoUrl: 'https://github.com/TanStack/query',
          repositoryDirectory: 'packages/query-core',
        }),
      ],
      totalFound: 3,
    });
    const t = text(await callTool('zustand'));
    expect(t).toContain('name: zustand');
    expect(t).toContain('name: jotai');
    expect(t).toContain("name: '@tanstack/query'");
    expect(t).toContain('repositoryDirectory: packages/query-core');
  });

  it('broad searches include pagination metadata and next-page hints', async () => {
    mockSearchPackage.mockResolvedValue({
      packages: [
        pkg({ name: 'zustand', repoUrl: 'https://github.com/pmndrs/zustand' }),
        pkg({ name: 'jotai', repoUrl: 'https://github.com/pmndrs/jotai' }),
      ],
      totalFound: 25,
    });
    const r = await callTool('state management');
    const t = text(r);
    expect(t).toContain('pagination:');
    expect(t).toContain('currentPage: 1');
    expect(t).toContain('totalPages: 3');
    expect(t).toContain('totalFound: 25');
    expect(t).toContain('Next: page=2');
    expect(t).toContain('Found 2 of 25 packages');
  });

  it('packages[] is a YAML sequence of objects with name, version, description, license, weeklyDownloads', async () => {
    mockSearchPackage.mockResolvedValue({
      packages: [
        pkg({
          name: 'express',
          version: '4.18.2',
          repoUrl: 'https://github.com/expressjs/express',
          description: 'Fast web framework',
          license: 'MIT',
          weeklyDownloads: 35000000,
        }),
      ],
      totalFound: 1,
    });
    const t = text(await callTool('express'));
    expect(t).toContain('name: express');
    expect(t).toContain('version: 4.18.2');
    expect(t).toContain('description: Fast web framework');
    expect(t).toContain('license: MIT');
    expect(t).toContain('weeklyDownloads: 35000000');
    expect(t).not.toContain('repoUrl:');
    expect(t).not.toContain('npmUrl:');
  });
});

describe('hints — exact / single result', () => {
  beforeEach(() => vi.clearAllMocks());

  it('includes Install hint with package name', async () => {
    mockSearchPackage.mockResolvedValue({
      packages: [
        pkg({ name: 'zod', repoUrl: 'https://github.com/colinhacks/zod' }),
      ],
      totalFound: 1,
    });
    const t = text(await callTool('zod'));
    expect(t).toContain('Install: npm install zod');
  });

  it('includes Browse source hint with owner and repo for GitHub packages', async () => {
    mockSearchPackage.mockResolvedValue({
      packages: [
        pkg({ name: 'zod', repoUrl: 'https://github.com/colinhacks/zod' }),
      ],
      totalFound: 1,
    });
    const t = text(await callTool('zod'));
    expect(t).toContain('ghViewRepoStructure');
    expect(t).toContain('owner=colinhacks');
    expect(t).toContain('repo=zod');
  });

  it('uses ghSearchRepos when repoUrl is null', async () => {
    mockSearchPackage.mockResolvedValue({
      packages: [pkg({ repoUrl: null })],
      totalFound: 1,
    });
    const t = text(await callTool('mypkg'));
    expect(t).toContain('ghSearchRepos');
    expect(t).not.toContain('ghViewRepoStructure');
  });

  it('uses ghSearchRepos for non-GitHub repo URLs', async () => {
    mockSearchPackage.mockResolvedValue({
      packages: [pkg({ repoUrl: 'https://gitlab.com/owner/repo' })],
      totalFound: 1,
    });
    const t = text(await callTool('mypkg'));
    expect(t).toContain('ghSearchRepos');
  });

  it('adds DEPRECATED prefix when package is deprecated', async () => {
    mockSearchPackage.mockResolvedValue({
      packages: [pkg({ repoUrl: 'https://github.com/owner/old' })],
      totalFound: 1,
    });
    vi.mocked(packageCommon.checkNpmDeprecation).mockResolvedValue({
      deprecated: true,
      message: 'Use new-pkg instead',
    });
    const t = text(await callTool('old'));
    expect(t).toContain('DEPRECATED');
    expect(t).toContain('Use new-pkg instead');
  });

  it('skips deprecation check for CDN fallback source', async () => {
    mockSearchPackage.mockResolvedValue({
      packages: [
        pkg({ source: 'cdn', repoUrl: 'https://github.com/owner/pkg' }),
      ],
      totalFound: 1,
    });
    await callTool('pkg');
    expect(packageCommon.checkNpmDeprecation).not.toHaveBeenCalled();
  });

  it('skips deprecation check for web fallback source', async () => {
    mockSearchPackage.mockResolvedValue({
      packages: [
        pkg({ source: 'web', repoUrl: 'https://github.com/owner/pkg' }),
      ],
      totalFound: 1,
    });
    await callTool('pkg');
    expect(packageCommon.checkNpmDeprecation).not.toHaveBeenCalled();
  });
});

describe('hints — keyword / multiple results', () => {
  beforeEach(() => vi.clearAllMocks());

  it('does NOT emit Install or Browse hints for a specific package', async () => {
    mockSearchPackage.mockResolvedValue({
      packages: [
        pkg({ name: 'obscure-pkg-a', repoUrl: 'https://github.com/a/a' }),
        pkg({ name: 'zustand', repoUrl: 'https://github.com/pmndrs/zustand' }),
        pkg({ name: 'jotai', repoUrl: 'https://github.com/pmndrs/jotai' }),
      ],
      totalFound: 3,
    });
    const t = text(await callTool('state management'));
    expect(t).not.toContain('npm install obscure-pkg-a');
    expect(t).not.toContain('ghViewRepoStructure owner=a repo=a');
  });

  it('tells agent to pick one and re-run with exact name', async () => {
    mockSearchPackage.mockResolvedValue({
      packages: [
        pkg({ name: 'pkg-a', repoUrl: 'https://github.com/a/a' }),
        pkg({ name: 'pkg-b', repoUrl: 'https://github.com/b/b' }),
      ],
      totalFound: 2,
    });
    const t = text(await callTool('state lib'));
    expect(t).toMatch(/exact|pick|re.?run|refine/i);
  });

  it('does not check deprecation for keyword results', async () => {
    mockSearchPackage.mockResolvedValue({
      packages: [
        pkg({ name: 'a', repoUrl: 'https://github.com/a/a' }),
        pkg({ name: 'b', repoUrl: 'https://github.com/b/b' }),
      ],
      totalFound: 2,
    });
    await callTool('state lib');
    expect(packageCommon.checkNpmDeprecation).not.toHaveBeenCalled();
  });
});

describe('hints — empty result', () => {
  beforeEach(() => vi.clearAllMocks());

  it('reports package not found', async () => {
    mockSearchPackage.mockResolvedValue({ packages: [], totalFound: 0 });
    const t = text(await callTool('no-such-pkg'));
    expect(t).toContain('Check spelling');
  });

  it('suggests hyphen→underscore variation (via hints.ts buildVariations)', async () => {
    mockSearchPackage.mockResolvedValue({ packages: [], totalFound: 0 });
    const t = text(await callTool('my-pkg'));
    expect(t).toContain('my_pkg');
  });

  it('suggests unscoped name for scoped packages (via hints.ts buildVariations)', async () => {
    mockSearchPackage.mockResolvedValue({ packages: [], totalFound: 0 });
    const t = text(await callTool('@scope/mypkg'));
    expect(t).toContain('mypkg');
  });
});

describe('hints — error recovery', () => {
  beforeEach(() => vi.clearAllMocks());

  it('propagates error hints from the npm layer', async () => {
    mockSearchPackage.mockResolvedValue({
      error: 'npm registry is unreachable.',
      hints: ['Use `ghSearchRepos` to find the source repo.'],
    });
    const t = text(await callTool('mypkg'));
    expect(t).toContain('ghSearchRepos');
  });

  it('isError=true on NpmSearchError', async () => {
    mockSearchPackage.mockResolvedValue({ error: 'fetch failed' });
    const r = await callTool('mypkg');
    expect(r.isError).toBe(true);
  });

  it('isError=true on thrown exception', async () => {
    mockSearchPackage.mockRejectedValue(new Error('network error'));
    const r = await callTool('mypkg');
    expect(r.isError).toBe(true);
    expect(text(r)).toContain('ghSearchRepos');
  });
});

describe('pagination — hasMore through searchPackages', () => {
  beforeEach(() => vi.clearAllMocks());

  it('sets hasMore:true when packages.length < totalFound', async () => {
    mockSearchPackage.mockResolvedValue({
      packages: [
        pkg({ name: 'zustand', repoUrl: 'https://github.com/pmndrs/zustand' }),
        pkg({ name: 'jotai', repoUrl: 'https://github.com/pmndrs/jotai' }),
      ],
      totalFound: 50, // API knows about 50, only 2 returned
    });

    const t = text(await callTool('state management'));
    expect(t).toContain('hasMore: true');
    expect(t).toContain('totalFound: 50');
  });

  it('sets pagination.hasMore:false when packages.length === totalFound', async () => {
    mockSearchPackage.mockResolvedValue({
      packages: [
        pkg({ name: 'zustand', repoUrl: 'https://github.com/pmndrs/zustand' }),
        pkg({ name: 'jotai', repoUrl: 'https://github.com/pmndrs/jotai' }),
      ],
      totalFound: 2, // exactly what was returned
    });

    const t = text(await callTool('zustand'));
    expect(t).toContain('hasMore: false');
  });
});

describe('bulk queries', () => {
  beforeEach(() => vi.clearAllMocks());

  it('processes multiple queries independently', async () => {
    mockSearchPackage
      .mockResolvedValueOnce({
        packages: [
          pkg({
            name: 'zustand',
            repoUrl: 'https://github.com/pmndrs/zustand',
          }),
        ],
        totalFound: 1,
      })
      .mockResolvedValueOnce({
        packages: [
          pkg({ name: 'jotai', repoUrl: 'https://github.com/pmndrs/jotai' }),
        ],
        totalFound: 1,
      });

    const r = await searchPackages({
      queries: [
        { ...BASE, id: 'q1', packageName: 'zustand' },
        { ...BASE, id: 'q2', packageName: 'jotai' },
      ] as never,
    });

    const t = text(r);
    expect(t).toContain('name: zustand');
    expect(t).toContain('name: jotai');
    expect(mockSearchPackage).toHaveBeenCalledTimes(2);
  });
});
