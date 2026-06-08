import { describe, it, expect, vi, beforeEach } from 'vitest';
import { searchPackages } from '../../src/tools/package_search/execution.js';
import * as packageCommon from '../../src/utils/package/common.js';

vi.mock('../../src/utils/package/common.js', () => ({
  searchPackage: vi.fn(),
  checkNpmDeprecation: vi.fn().mockResolvedValue(null),
}));

const mockSearchPackage = vi.mocked(packageCommon.searchPackage);

describe('package_search execution branches', () => {
  const baseQuery = {
    mainResearchGoal: 'Test',
    researchGoal: 'Find package',
    reasoning: 'Testing',
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('input validation', () => {
    it('should return error when name is missing', async () => {
      const result = await searchPackages({
        queries: [{ ...baseQuery } as never],
      });

      expect(result.content).toBeDefined();
      const content = Array.isArray(result.content)
        ? result.content
        : [{ type: 'text', text: JSON.stringify(result.content) }];
      const text = content
        .map((c: { text?: string }) => c.text)
        .join('')
        .toLowerCase();
      expect(text).toContain('required');
      expect(mockSearchPackage).not.toHaveBeenCalled();
    });

    it('should return error when name is empty', async () => {
      const result = await searchPackages({
        queries: [
          {
            ...baseQuery,
            name: '',
          } as never,
        ],
      });

      expect(result.content).toBeDefined();
      expect(mockSearchPackage).not.toHaveBeenCalled();
    });
  });

  describe('parseRepoInfo - repoUrl does not match github', () => {
    it('should return package without owner/repo when repository URL is not from supported hosts', async () => {
      mockSearchPackage.mockResolvedValue({
        packages: [
          {
            path: 'some-pkg',
            version: '1.0.0',
            repoUrl: 'https://example.com/owner/repo',
            mainEntry: null,
            typeDefinitions: null,
          },
        ],
        totalFound: 1,
      });

      const result = await searchPackages({
        queries: [
          {
            ...baseQuery,
            id: 'test:1',
            name: 'some-pkg',
          } as never,
        ],
      });

      expect(result.content).toBeDefined();
      const content = Array.isArray(result.content)
        ? result.content
        : [{ type: 'text', text: String(result.content) }];
      const text = content.map((c: { text?: string }) => c.text ?? '').join('');
      expect(text).toContain('some-pkg');
      expect(text).toContain('1.0.0');
      expect(result.isError).not.toBe(true);
    });
  });

  describe('generateSuccessHints branches', () => {
    it('should add deprecated hint when package is deprecated (line 147)', async () => {
      mockSearchPackage.mockResolvedValue({
        packages: [
          {
            path: 'deprecated-pkg',
            version: '1.0.0',
            repoUrl: 'https://github.com/owner/repo',
            mainEntry: null,
            typeDefinitions: null,
          },
        ],
        totalFound: 1,
      });
      vi.mocked(packageCommon.checkNpmDeprecation).mockResolvedValue({
        deprecated: true,
        message: 'Use new-pkg instead',
      });

      const result = await searchPackages({
        queries: [
          {
            ...baseQuery,
            name: 'deprecated-pkg',
          } as never,
        ],
      });

      const text = (result.content as { text?: string }[])?.[0]?.text ?? '';
      expect(text).toContain('DEPRECATED');
      expect(text).toContain('Use new-pkg instead');
    });
  });

  describe('network error recovery hints', () => {
    it('PackageSearchError emits githubSearchRepositories recovery hint', async () => {
      mockSearchPackage.mockResolvedValue({
        error: 'Failed to fetch after 2 attempts: fetch failed',
      });

      const result = await searchPackages({
        queries: [{ ...baseQuery, id: 'err:1', name: 'react' } as never],
      });

      expect(result.isError).toBe(true);
      const text = (result.content as { text?: string }[])?.[0]?.text ?? '';
      expect(text).toContain('githubSearchRepositories');
    });

    it('PackageSearchError for unreachable registry emits unreachable hint', async () => {
      mockSearchPackage.mockResolvedValue({
        error: 'NPM view failed: network timeout',
      });

      const result = await searchPackages({
        queries: [{ ...baseQuery, id: 'err:2', name: 'lodash' } as never],
      });

      expect(result.isError).toBe(true);
      const text = (result.content as { text?: string }[])?.[0]?.text ?? '';
      expect(text).toContain('unreachable');
    });

    it('thrown error emits githubSearchRepositories recovery hint', async () => {
      mockSearchPackage.mockRejectedValue(new Error('fetch failed'));

      const result = await searchPackages({
        queries: [{ ...baseQuery, id: 'err:3', name: 'axios' } as never],
      });

      const text = (result.content as { text?: string }[])?.[0]?.text ?? '';
      expect(text).toContain('githubSearchRepositories');
    });
  });

  describe('npm.ts hints propagate through execution.ts (integration boundary)', () => {
    it('passes hints from PackageSearchError through to the response', async () => {
      mockSearchPackage.mockResolvedValue({
        error: 'NPM registry search failed: fetch failed',
        hints: [
          'npm registry is unreachable on all endpoints (exact lookup + /-/v1/search).',
          'Use `githubSearchRepositories` to find the source repo directly by package name or domain terms.',
        ],
      });

      const result = await searchPackages({
        queries: [
          { ...baseQuery, id: 'hint:1', name: 'octocode-mcp' } as never,
        ],
      });

      expect(result.isError).toBe(true);
      const text = (result.content as { text?: string }[])?.[0]?.text ?? '';
      expect(text).toContain('unreachable');
      expect(text).toContain('githubSearchRepositories');
    });

    it('packages returned via fallback path are shaped correctly', async () => {
      mockSearchPackage.mockResolvedValue({
        packages: [
          {
            name: '@modelcontextprotocol/sdk',
            version: '1.0.0',
            description: 'MCP TypeScript SDK',
            repoUrl: 'https://github.com/modelcontextprotocol/typescript-sdk',
            mainEntry: null,
            typeDefinitions: null,
          },
        ],
        totalFound: 1,
      });

      const result = await searchPackages({
        queries: [
          {
            ...baseQuery,
            id: 'fallback:1',
            name: '@modelcontextprotocol/sdk',
          } as never,
        ],
      });

      expect(result.isError).not.toBe(true);
      const text = (result.content as { text?: string }[])?.[0]?.text ?? '';
      expect(text).toContain('@modelcontextprotocol/sdk');
      expect(text).toContain('1.0.0');
      expect(text).toContain('modelcontextprotocol');
    });
  });

  describe('research handoff output', () => {
    it('adds repository, entrypoints, researchTargets, and truncates descriptions', async () => {
      mockSearchPackage.mockResolvedValue({
        packages: [
          {
            name: '@scope/pkg',
            version: '1.0.0',
            description: 'D'.repeat(250),
            repoUrl: 'https://github.com/owner/repo',
            mainEntry: 'dist/index.cjs',
            moduleEntry: 'dist/index.mjs',
            typeDefinitions: 'dist/index.d.ts',
            packageType: 'module',
            repositoryDirectory: 'packages/pkg',
            exports: ['./dist/index.mjs', './dist/index.d.ts'],
            dependencies: { leftpad: '^1.0.0' },
            peerDependencies: { react: '^18.0.0' },
          },
        ],
        totalFound: 1,
      });

      const result = await searchPackages({
        queries: [
          { ...baseQuery, id: 'handoff:1', name: '@scope/pkg' } as never,
        ],
      });

      expect(result.isError).not.toBe(true);
      const text = (result.content as { text?: string }[])?.[0]?.text ?? '';
      expect(text).toContain('repository:');
      expect(text).toContain('sourceRoot: "packages/pkg"');
      expect(text).toContain('entrypoints:');
      expect(text).toContain('researchTargets:');
      expect(text).toContain('githubGetFileContent');
      expect(text).toContain('packages/pkg/dist/index.cjs');
      expect(text).not.toContain('leftpad');
      expect(text).not.toContain('peerDependencies');
      expect(text).toContain(`${'D'.repeat(197)}...`);
    });

    it('exports supersedes main, surfaces bin, and never duplicates the "." entry', async () => {
      mockSearchPackage.mockResolvedValue({
        packages: [
          {
            name: 'clitool',
            version: '2.0.0',
            repoUrl: 'https://github.com/owner/clitool',
            mainEntry: './dist/index.cjs', // legacy fallback; should be superseded
            typeDefinitions: './dist/index.d.ts',
            // collapsed exports (one per subpath) as produced upstream
            exports: ['. → ./dist/index.js', './sub → ./dist/sub.js'],
            bin: ['clitool → ./bin/cli.js'],
          },
        ],
        totalFound: 1,
      });

      const result = await searchPackages({
        queries: [{ ...baseQuery, id: 'cli:1', name: 'clitool' } as never],
      });

      const text = (result.content as { text?: string }[])?.[0]?.text ?? '';
      // "." promoted to main from exports (supersedes the legacy ./dist/index.cjs)
      expect(text).toContain('main: "./dist/index.js"');
      expect(text).not.toContain('./dist/index.cjs');
      // exports lists only the OTHER subpaths — "." is not shown twice
      expect(text).toContain('./sub → ./dist/sub.js');
      expect(text).not.toContain('". → ./dist/index.js"');
      // bin (CLI code location) is surfaced
      expect(text).toContain('clitool → ./bin/cli.js');
    });

    it('keeps non-GitHub repository URLs without GitHub research targets', async () => {
      mockSearchPackage.mockResolvedValue({
        packages: [
          {
            name: 'non-gh',
            version: '1.0.0',
            repoUrl: 'https://example.com/owner/repo',
            mainEntry: 'index.js',
            typeDefinitions: null,
          },
        ],
        totalFound: 1,
      });

      const result = await searchPackages({
        queries: [{ ...baseQuery, id: 'handoff:2', name: 'non-gh' } as never],
      });

      const text = (result.content as { text?: string }[])?.[0]?.text ?? '';
      expect(text).toContain('url: "https://example.com/owner/repo"');
      expect(text).not.toContain('researchTargets');
      expect(text).not.toContain('owner: "owner"');
    });
  });

  describe('generateSuccessHints — null repoUrl branch', () => {
    it('emits githubSearchRepositories hint when repoUrl is null in npm manifest', async () => {
      mockSearchPackage.mockResolvedValue({
        packages: [
          {
            name: 'no-repo-pkg',
            version: '2.0.0',
            description: 'Package with no repository field',
            repoUrl: null,
            mainEntry: null,
            typeDefinitions: null,
          },
        ],
        totalFound: 1,
      });

      const result = await searchPackages({
        queries: [{ ...baseQuery, name: 'no-repo-pkg' } as never],
      });

      expect(result.isError).not.toBe(true);
      const text = (result.content as { text?: string }[])?.[0]?.text ?? '';
      expect(text).toContain('No repository URL in npm manifest');
      expect(text).toContain('githubSearchRepositories');
    });
  });
});
