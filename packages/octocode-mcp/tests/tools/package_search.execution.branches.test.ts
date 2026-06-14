/**
 * Contract tests for the packageSearch execution layer.
 *
 * Tests are written against OUTPUT CONTRACTS, not implementation internals.
 * Every test mocks `searchPackage` (the npm API boundary) and asserts:
 *   - the YAML/text content shape
 *   - the hints behaviour
 *   - the evidence flags
 *
 * Format contract: packages[] is a list of strings "name url[ sourceRoot]"
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { searchPackages } from '../../../octocode-tools-core/src/tools/package_search/execution.js';
import * as packageCommon from '../../../octocode-tools-core/src/utils/package/common.js';

vi.mock('../../../octocode-tools-core/src/utils/package/common.js', () => ({
  searchPackage: vi.fn(),
  checkNpmDeprecation: vi.fn().mockResolvedValue(null),
}));

const mockSearchPackage = vi.mocked(packageCommon.searchPackage);

// ─── fixtures ────────────────────────────────────────────────────────────────

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

// ─── input validation ────────────────────────────────────────────────────────

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

// ─── output format: string list ──────────────────────────────────────────────

describe('output format — "name url[ sourceRoot]" string list', () => {
  beforeEach(() => vi.clearAllMocks());

  it('flat repo → "name https://github.com/owner/repo"', async () => {
    mockSearchPackage.mockResolvedValue({
      packages: [
        pkg({ name: 'zod', repoUrl: 'https://github.com/colinhacks/zod' }),
      ],
      totalFound: 1,
    });
    const t = text(await callTool('zod'));
    expect(t).toContain('zod https://github.com/colinhacks/zod');
    expect(t).not.toContain('zod https://github.com/colinhacks/zod '); // no trailing sourceRoot
  });

  it('monorepo → "name url sourceRoot"', async () => {
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
    expect(t).toContain(
      'react https://github.com/facebook/react packages/react'
    );
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
    expect(t).toContain('pkg-a https://github.com/org/mono packages/pkg-a');
    expect(t).not.toContain('./packages');
  });

  it('non-GitHub repoUrl → included as-is, no sourceRoot', async () => {
    mockSearchPackage.mockResolvedValue({
      packages: [pkg({ repoUrl: 'https://gitlab.com/owner/repo' })],
      totalFound: 1,
    });
    const t = text(await callTool('mypkg'));
    expect(t).toContain('mypkg https://gitlab.com/owner/repo');
  });

  it('null repoUrl → just "name"', async () => {
    mockSearchPackage.mockResolvedValue({
      packages: [pkg({ repoUrl: null })],
      totalFound: 1,
    });
    const t = text(await callTool('mypkg'));
    expect(t).toContain('mypkg');
    // no trailing URL
    expect(t).not.toContain('mypkg https://');
  });

  it('multiple packages → one string line each', async () => {
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
    expect(t).toContain('zustand https://github.com/pmndrs/zustand');
    expect(t).toContain('jotai https://github.com/pmndrs/jotai');
    expect(t).toContain(
      '@tanstack/query https://github.com/TanStack/query packages/query-core'
    );
  });

  it('packages[] is a YAML sequence of strings, not objects', async () => {
    mockSearchPackage.mockResolvedValue({
      packages: [
        pkg({
          name: 'express',
          repoUrl: 'https://github.com/expressjs/express',
        }),
      ],
      totalFound: 1,
    });
    const t = text(await callTool('express'));
    // must NOT contain any YAML object keys that would indicate an object shape
    expect(t).not.toContain('repoUrl:');
    expect(t).not.toContain('repositoryDirectory:');
    expect(t).not.toContain('npmUrl:');
    expect(t).not.toContain('version:');
    expect(t).not.toContain('weeklyDownloads:');
  });
});

// ─── evidence flags ───────────────────────────────────────────────────────────

describe('evidence flags', () => {
  beforeEach(() => vi.clearAllMocks());

  it('answerReady:true and complete:true when package found', async () => {
    mockSearchPackage.mockResolvedValue({
      packages: [pkg()],
      totalFound: 1,
    });
    const t = text(await callTool('mypkg'));
    expect(t).toContain('answerReady: true');
    expect(t).toContain('complete: true');
  });

  it('answerReady:false when no packages found', async () => {
    mockSearchPackage.mockResolvedValue({ packages: [], totalFound: 0 });
    const t = text(await callTool('no-such-pkg'));
    expect(t).toContain('answerReady: false');
  });
});

// ─── hints — exact / single result ───────────────────────────────────────────

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
    expect(t).toContain('githubViewRepoStructure');
    expect(t).toContain('owner=colinhacks');
    expect(t).toContain('repo=zod');
  });

  it('uses githubSearchRepositories when repoUrl is null', async () => {
    mockSearchPackage.mockResolvedValue({
      packages: [pkg({ repoUrl: null })],
      totalFound: 1,
    });
    const t = text(await callTool('mypkg'));
    expect(t).toContain('githubSearchRepositories');
    expect(t).not.toContain('githubViewRepoStructure');
  });

  it('uses githubSearchRepositories for non-GitHub repo URLs', async () => {
    mockSearchPackage.mockResolvedValue({
      packages: [pkg({ repoUrl: 'https://gitlab.com/owner/repo' })],
      totalFound: 1,
    });
    const t = text(await callTool('mypkg'));
    expect(t).toContain('githubSearchRepositories');
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

// ─── hints — keyword / multiple results ──────────────────────────────────────

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
    // Should NOT say "npm install obscure-pkg-a" (first result bias)
    expect(t).not.toContain('npm install obscure-pkg-a');
    expect(t).not.toContain('githubViewRepoStructure owner=a repo=a');
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
    // hint should guide toward exact name lookup
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

// ─── hints — empty result ─────────────────────────────────────────────────────

describe('hints — empty result', () => {
  beforeEach(() => vi.clearAllMocks());

  it('reports package not found', async () => {
    mockSearchPackage.mockResolvedValue({ packages: [], totalFound: 0 });
    const t = text(await callTool('no-such-pkg'));
    // hints.ts empty handler: "Package '...' not found on npm."
    expect(t).toContain("Package 'no-such-pkg' not found on npm.");
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

// ─── hints — error recovery ───────────────────────────────────────────────────

describe('hints — error recovery', () => {
  beforeEach(() => vi.clearAllMocks());

  it('propagates error hints from the npm layer', async () => {
    mockSearchPackage.mockResolvedValue({
      error: 'npm registry is unreachable.',
      hints: ['Use `githubSearchRepositories` to find the source repo.'],
    });
    const t = text(await callTool('mypkg'));
    expect(t).toContain('githubSearchRepositories');
  });

  it('isError=true on PackageSearchError', async () => {
    mockSearchPackage.mockResolvedValue({ error: 'fetch failed' });
    const r = await callTool('mypkg');
    expect(r.isError).toBe(true);
  });

  it('isError=true on thrown exception', async () => {
    mockSearchPackage.mockRejectedValue(new Error('network error'));
    const r = await callTool('mypkg');
    expect(r.isError).toBe(true);
    expect(text(r)).toContain('githubSearchRepositories');
  });
});

// ─── pagination ─────────────────────────────────────────────────────────────────

describe('pagination — hasMore through searchPackages', () => {
  beforeEach(() => vi.clearAllMocks());

  it('sets hasMore:true and complete:false when packages.length < totalFound', async () => {
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
    expect(t).toContain('complete: false');
  });

  it('no hasMore when packages.length === totalFound (complete page)', async () => {
    mockSearchPackage.mockResolvedValue({
      packages: [
        pkg({ name: 'zustand', repoUrl: 'https://github.com/pmndrs/zustand' }),
        pkg({ name: 'jotai', repoUrl: 'https://github.com/pmndrs/jotai' }),
      ],
      totalFound: 2, // exactly what was returned
    });

    const t = text(await callTool('zustand'));
    expect(t).not.toContain('hasMore');
    expect(t).toContain('complete: true');
  });
});

// ─── bulk queries ─────────────────────────────────────────────────────────────────

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
    expect(t).toContain('zustand https://github.com/pmndrs/zustand');
    expect(t).toContain('jotai https://github.com/pmndrs/jotai');
    expect(mockSearchPackage).toHaveBeenCalledTimes(2);
  });
});
