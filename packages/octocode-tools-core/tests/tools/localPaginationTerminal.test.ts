import { afterEach, describe, expect, it, vi } from 'vitest';

import type { LocalSearchCodeFile } from '@octocodeai/octocode-core/types';
import { findFiles } from '../../src/tools/local_find_files/findFiles.js';
import { viewStructure } from '../../src/tools/local_view_structure/local_view_structure.js';
import { buildSearchResult } from '../../src/tools/local_ripgrep/ripgrepResultBuilder/buildResult.js';
import {
  resetContextUtilsNativeLoaderForTesting,
  setContextUtilsNativeLoaderForTesting,
} from '../../src/utils/contextUtils.js';

type NativeContextUtilsModule = typeof import('@octocodeai/octocode-engine');

function installEntries(count: number): void {
  const entries = Array.from({ length: count }, (_, index) => ({
    name: `file-${index}.ts`,
    path: `${process.cwd()}/file-${index}.ts`,
    type: 'file' as const,
    size: 1,
  }));
  setContextUtilsNativeLoaderForTesting(
    () =>
      ({
        queryFileSystem: vi.fn().mockResolvedValue({
          entries,
          totalDiscovered: entries.length,
          wasCapped: false,
          skipped: 0,
          permissionDenied: 0,
          warnings: [],
        }),
      }) as unknown as NativeContextUtilsModule
  );
}

function expectTerminalPage(result: Record<string, unknown>): void {
  expect(result.terminalLimit).toBe(true);
  expect(result.pagination).toMatchObject({
    currentPage: 1000,
    hasMore: true,
  });
  expect(result.pagination).not.toHaveProperty('nextPage');
  expect(result.next).not.toHaveProperty('nextPage');
}

afterEach(() => {
  resetContextUtilsNativeLoaderForTesting();
});

describe('localSearch schema page ceiling', () => {
  it('marks files and tree page 1000 terminal without page 1001 continuations', async () => {
    installEntries(1001);
    const files = await findFiles({
      path: process.cwd(),
      itemsPerPage: 1,
      page: 1000,
      limit: 2000,
      detail: 'basic',
    } as never);
    expectTerminalPage(files as unknown as Record<string, unknown>);

    const tree = await viewStructure({
      path: process.cwd(),
      itemsPerPage: 1,
      page: 1000,
      limit: 2000,
      detail: 'full',
    } as never);
    expectTerminalPage(tree as unknown as Record<string, unknown>);
  });

  it.each(['rg', 'structural'] as const)(
    'marks %s page 1000 terminal without a page 1001 continuation',
    async engine => {
      const files = Array.from({ length: 1001 }, (_, index) => ({
        path: `${process.cwd()}/file-${index}.ts`,
        matches: [{ line: 1, value: 'needle' }],
        matchCount: 1,
      })) as LocalSearchCodeFile[];
      const result = await buildSearchResult(
        files,
        {
          path: process.cwd(),
          pattern: 'needle',
          page: 1000,
          itemsPerPage: 1,
          maxMatchesPerFile: 10,
          output: 'content',
          sort: 'path',
        } as never,
        engine,
        []
      );

      expectTerminalPage(result as unknown as Record<string, unknown>);
    }
  );

  it('makes files and tree pre-pagination limits explicit and expandable', async () => {
    installEntries(3);
    const files = (await findFiles({
      path: process.cwd(),
      itemsPerPage: 1,
      page: 1,
      limit: 1,
      detail: 'basic',
    } as never)) as unknown as Record<string, unknown>;
    expect(files).toMatchObject({
      truncated: true,
      partialReasons: ['limit'],
      totalAvailable: 3,
    });
    expect(files.terminalLimit).toBeUndefined();
    expect(files.next).toMatchObject({
      expandLimit: {
        tool: 'local.files',
        query: { limit: 2, page: 1 },
      },
    });

    const tree = (await viewStructure({
      path: process.cwd(),
      itemsPerPage: 1,
      page: 1,
      limit: 1,
      detail: 'full',
    } as never)) as unknown as Record<string, unknown>;
    expect(tree).toMatchObject({
      truncated: true,
      partialReasons: ['limit'],
      totalAvailable: 3,
    });
    expect(tree.terminalLimit).toBeUndefined();
    expect(tree.next).toMatchObject({
      expandLimit: {
        tool: 'local.tree',
        query: { limit: 2, page: 1 },
      },
    });
  });

  it('makes a structural file-scan cap expandable below the public maximum', async () => {
    const result = (await buildSearchResult(
      [
        {
          path: `${process.cwd()}/one.ts`,
          matches: [{ line: 1, value: 'needle' }],
          matchCount: 1,
        } as LocalSearchCodeFile,
      ],
      {
        path: process.cwd(),
        pattern: 'needle',
        maxFiles: 1,
        output: 'content',
        sort: 'path',
      } as never,
      'structural',
      [],
      { capReached: true } as never
    )) as unknown as Record<string, unknown>;

    expect(result).toMatchObject({
      truncated: true,
      partialReasons: ['maxFiles'],
      next: {
        expandScan: {
          tool: 'local.text',
          query: { maxFiles: 2, page: 1 },
        },
      },
    });
    expect(result.terminalLimit).toBeUndefined();
  });

  it.each(['rg', 'structural'] as const)(
    'marks %s matchPage 1000 terminal without matchPage 1001',
    async engine => {
      const result = (await buildSearchResult(
        [
          {
            path: `${process.cwd()}/many.ts`,
            matches: Array.from({ length: 1001 }, (_, index) => ({
              line: index + 1,
              value: 'needle',
            })),
            matchCount: 1001,
          } as LocalSearchCodeFile,
        ],
        {
          path: process.cwd(),
          pattern: 'needle',
          matchPage: 1000,
          maxMatchesPerFile: 1,
          output: 'content',
          sort: 'path',
        } as never,
        engine,
        []
      )) as unknown as Record<string, unknown>;

      expect(result.terminalLimit).toBe(true);
      expect(result.next).not.toHaveProperty('nextMatchPage');
      const files = result.files as Array<{
        pagination?: Record<string, unknown>;
      }>;
      expect(files[0]?.pagination).toMatchObject({
        currentPage: 1000,
        hasMore: true,
      });
      expect(files[0]?.pagination).not.toHaveProperty('nextMatchPage');
    }
  );
});
