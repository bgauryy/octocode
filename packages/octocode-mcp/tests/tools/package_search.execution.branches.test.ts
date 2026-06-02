/**
 * Branch coverage tests for package_search/execution.ts
 * Targets: missing ecosystem/name validation, parseRepoInfo when repoUrl doesn't match
 */

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
        queries: [{ ...baseQuery, ecosystem: 'npm' } as never],
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

    it('should reject non-npm ecosystems without searching the registry', async () => {
      const result = await searchPackages({
        queries: [
          { ...baseQuery, name: 'requests', ecosystem: 'pypi' } as never,
        ],
      });

      const text = (result.content as { text?: string }[])?.[0]?.text ?? '';
      expect(result.isError).toBe(true);
      expect(text).toContain('Only ecosystem');
      expect(text).toContain('npm');
      expect(mockSearchPackage).not.toHaveBeenCalled();
    });

    it('should return error when name is empty', async () => {
      const result = await searchPackages({
        queries: [
          {
            ...baseQuery,
            ecosystem: 'npm',
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
        ecosystem: 'npm',
        totalFound: 1,
      });

      const result = await searchPackages({
        queries: [
          {
            ...baseQuery,
            id: 'test:1',
            ecosystem: 'npm',
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
        ecosystem: 'npm',
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
            ecosystem: 'npm',
            name: 'deprecated-pkg',
          } as never,
        ],
      });

      const text = (result.content as { text?: string }[])?.[0]?.text ?? '';
      expect(text).toContain('DEPRECATED');
      expect(text).toContain('Use new-pkg instead');
    });
  });

  describe('packageSearch verbosity shaping', () => {
    it('concise keeps the top three package candidates', async () => {
      const { applyPackageSearchVerbosity } =
        await import('../../src/tools/package_search/execution.js');

      const out = applyPackageSearchVerbosity(
        {
          data: {
            packages: [
              {
                path: 'one',
                version: '1.0.0',
                repoUrl: null,
                mainEntry: null,
                typeDefinitions: null,
              },
              {
                path: 'two',
                version: '2.0.0',
                repoUrl: null,
                mainEntry: null,
                typeDefinitions: null,
              },
              {
                path: 'three',
                version: '3.0.0',
                repoUrl: null,
                mainEntry: null,
                typeDefinitions: null,
              },
              {
                path: 'four',
                version: '4.0.0',
                repoUrl: null,
                mainEntry: null,
                typeDefinitions: null,
              },
            ],
            totalFound: 4,
          },
          extraHints: [],
        },
        { name: 'pkg', ecosystem: 'npm', verbosity: 'concise' } as never
      );

      expect(out.data.packages).toHaveLength(3);
      expect(
        (out.data.packages as Array<Record<string, unknown>>).map(p => p.name)
      ).toEqual(['one', 'two', 'three']);
    });
  });
});
