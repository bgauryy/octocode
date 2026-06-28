import { describe, expect, it } from 'vitest';

import {
  buildPackagePagination,
  compactPackageRepositories,
} from '../../../src/tools/package_search/execution.js';
import type { z } from 'zod';
import type { NpmPackageQuerySchema } from '@octocodeai/octocode-core/schemas';

type NpmSearchQuery = z.input<typeof NpmPackageQuerySchema>;

const query = (page?: number): NpmSearchQuery =>
  ({ packageName: 'react', ...(page ? { page } : {}) }) as NpmSearchQuery;

describe('npmSearch keyword pagination (full-page heuristic)', () => {
  it('a FULL keyword page reports hasMore:true + nextPage even when totalFound understates the grand total', () => {
    // CLI keyword search returns totalFound == this-page count (10), but a full
    // page means more results exist. Must NOT report hasMore:false.
    const pg = buildPackagePagination(query(1), 10, 10, true);
    expect(pg.returned).toBe(10);
    expect(pg.hasMore).toBe(true);
    expect(pg.nextPage).toBe(2);
  });

  it('a PARTIAL last keyword page reports hasMore:false (no false continuation)', () => {
    const pg = buildPackagePagination(query(2), 14, 4, true);
    expect(pg.returned).toBe(4);
    expect(pg.hasMore).toBe(false);
    expect(pg.nextPage).toBeUndefined();
  });

  it('honors a real grand total when the registry exposes it (page not full)', () => {
    // Registry path: totalFound is the true total (123), page returns full 10.
    const pg = buildPackagePagination(query(1), 123, 10, true);
    expect(pg.hasMore).toBe(true);
    expect(pg.totalPages).toBe(Math.ceil(123 / 10));
    expect(pg.nextPage).toBe(2);
  });

  it('exact (non-keyword) single-package lookup stays single-page', () => {
    const pg = buildPackagePagination(query(1), 1, 1, false);
    expect(pg.perPage).toBe(1);
    expect(pg.hasMore).toBe(false);
    expect(pg.totalPages).toBe(1);
  });
});

describe('npmSearch package repository compaction', () => {
  it('hoists repeated GitHub next calls into a shared repository table', () => {
    const compacted = compactPackageRepositories([
      {
        name: 'octocode',
        repository: 'https://github.com/bgauryy/octocode-mcp',
        next: { old: true },
      },
      {
        name: 'octocode-cli',
        repository: 'https://github.com/bgauryy/octocode-mcp',
        next: { old: true },
      },
      {
        name: 'fuse.js',
        repository: 'https://github.com/krisk/Fuse',
        next: { old: true },
      },
    ]);

    expect(compacted.packages).toEqual([
      { name: 'octocode', repositoryId: 'r1' },
      { name: 'octocode-cli', repositoryId: 'r1' },
      { name: 'fuse.js', repositoryId: 'r2' },
    ]);
    expect(Object.keys(compacted.repositories ?? {})).toEqual(['r1', 'r2']);
    expect(compacted.repositories?.r1).toMatchObject({
      repository: 'https://github.com/bgauryy/octocode-mcp',
      owner: 'bgauryy',
      repo: 'octocode-mcp',
      next: {
        viewRepoStructure: {
          tool: 'ghViewRepoStructure',
          query: { owner: 'bgauryy', repo: 'octocode-mcp' },
        },
        searchCode: {
          tool: 'ghSearchCode',
          query: { owner: 'bgauryy', repo: 'octocode-mcp' },
        },
        cloneRepo: {
          tool: 'ghCloneRepo',
          query: { owner: 'bgauryy', repo: 'octocode-mcp' },
        },
      },
    });
  });
});
