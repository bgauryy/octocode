import { describe, it, expect } from 'vitest';
import {
  DEFAULT_PAGE_SIZE,
  STRUCTURE_PAGE_SIZE,
  BulkRipgrepQuerySchema,
  BulkFindFilesSchema,
  BulkViewStructureSchema,
} from '../../src/scheme/localSchemaOverlay.js';
import {
  GitHubReposSearchBulkQueryLocalSchema,
  GitHubCodeSearchBulkQueryLocalSchema,
  GitHubViewRepoStructureBulkQueryLocalSchema,
  PackageSearchBulkQueryLocalSchema,
} from '../../src/scheme/remoteSchemaOverlay.js';

describe('Page size constants', () => {
  it('DEFAULT_PAGE_SIZE is 20', () => {
    expect(DEFAULT_PAGE_SIZE).toBe(20);
  });

  it('STRUCTURE_PAGE_SIZE is 100', () => {
    expect(STRUCTURE_PAGE_SIZE).toBe(100);
  });
});

describe('GitHub search: page-based pagination, no legacy fields', () => {
  it('defaults page to 1 when omitted', () => {
    const parsed = GitHubReposSearchBulkQueryLocalSchema.parse({
      queries: [{ keywordsToSearch: ['x'] }],
    });
    const q = parsed.queries[0] as Record<string, unknown>;
    expect(q.page).toBe(1);
  });

  it('accepts explicit page > 1', () => {
    const parsed = GitHubReposSearchBulkQueryLocalSchema.parse({
      queries: [{ keywordsToSearch: ['x'], page: 3 }],
    });
    const q = parsed.queries[0] as Record<string, unknown>;
    expect(q.page).toBe(3);
  });

  it('does not expose itemsPerPage or githubAPILimit', () => {
    const parsed = GitHubCodeSearchBulkQueryLocalSchema.parse({
      queries: [{ keywordsToSearch: ['x'] }],
    });
    const q = parsed.queries[0] as Record<string, unknown>;
    expect('itemsPerPage' in q).toBe(false);
    expect('githubAPILimit' in q).toBe(false);
    expect('limit' in q).toBe(false);
  });
});

describe('Local tools: page-based pagination, legacy names gone', () => {
  const q0 = (
    schema: { parse: (v: unknown) => { queries: unknown[] } },
    query: unknown
  ) => schema.parse({ queries: [query] }).queries[0] as Record<string, unknown>;

  it('ripgrep: accepts page, no itemsPerPage, no filesPerPage', () => {
    const q = q0(BulkRipgrepQuerySchema, {
      pattern: 'x',
      path: '.',
      page: 2,
    });
    expect(q.page).toBe(2);
    expect('itemsPerPage' in q).toBe(false);
    expect('filesPerPage' in q).toBe(false);
    expect('matchesPerPage' in q).toBe(false);
    expect('filePageNumber' in q).toBe(false);
  });

  it('localFindFiles: accepts page, no filesPerPage', () => {
    const q = q0(BulkFindFilesSchema, { path: '.', name: '*.ts', page: 2 });
    expect(q.page).toBe(2);
    expect('filesPerPage' in q).toBe(false);
    expect('itemsPerPage' in q).toBe(false);
  });

  it('localViewStructure: accepts page, no entriesPerPage', () => {
    const q = q0(BulkViewStructureSchema, { path: '.', page: 2 });
    expect(q.page).toBe(2);
    expect('entriesPerPage' in q).toBe(false);
    expect('itemsPerPage' in q).toBe(false);
  });

  it('githubViewRepoStructure: accepts page, no entriesPerPage', () => {
    const q = q0(GitHubViewRepoStructureBulkQueryLocalSchema, {
      owner: 'o',
      repo: 'r',
      page: 2,
    });
    expect(q.page).toBe(2);
    expect('entriesPerPage' in q).toBe(false);
    expect('itemsPerPage' in q).toBe(false);
  });

  it('packageSearch: accepts page, no searchLimit', () => {
    const q = q0(PackageSearchBulkQueryLocalSchema, { name: 'react', page: 2 });
    expect(q.page).toBe(2);
    expect('searchLimit' in q).toBe(false);
    expect('itemsPerPage' in q).toBe(false);
  });
});
