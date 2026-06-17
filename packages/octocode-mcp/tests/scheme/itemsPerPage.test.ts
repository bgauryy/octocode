import { describe, it, expect } from 'vitest';
import {
  GITHUB_SEARCH_MAX_LIMIT,
  GITHUB_STRUCTURE_MAX_ENTRIES_PER_PAGE,
  LOCAL_DEFAULT_FILES_PER_PAGE,
  PR_CONTENT_DEFAULT_ITEMS_PER_PAGE,
} from '../../../octocode-tools-core/src/config.js';
import { LocalRipgrepBulkQuerySchema } from '../../../octocode-tools-core/src/tools/local_ripgrep/scheme.js';
import { LocalFindFilesBulkQuerySchema } from '../../../octocode-tools-core/src/tools/local_find_files/scheme.js';
import { LocalViewStructureBulkQuerySchema } from '../../../octocode-tools-core/src/tools/local_view_structure/scheme.js';
import { GitHubReposSearchBulkQueryLocalSchema } from '../../../octocode-tools-core/src/tools/github_search_repos/scheme.js';
import { GitHubCodeSearchBulkQueryLocalSchema } from '../../../octocode-tools-core/src/tools/github_search_code/scheme.js';
import { GitHubViewRepoStructureBulkQueryLocalSchema } from '../../../octocode-tools-core/src/tools/github_view_repo_structure/scheme.js';
import { NpmSearchBulkQueryLocalSchema } from '../../../octocode-tools-core/src/tools/package_search/scheme.js';

describe('Pagination config constants', () => {
  it('LOCAL_DEFAULT_FILES_PER_PAGE is 20', () => {
    expect(LOCAL_DEFAULT_FILES_PER_PAGE).toBe(20);
  });

  it('PR_CONTENT_DEFAULT_ITEMS_PER_PAGE is 20', () => {
    expect(PR_CONTENT_DEFAULT_ITEMS_PER_PAGE).toBe(20);
  });

  it('GITHUB_SEARCH_MAX_LIMIT is 100', () => {
    expect(GITHUB_SEARCH_MAX_LIMIT).toBe(100);
  });

  it('GITHUB_STRUCTURE_MAX_ENTRIES_PER_PAGE is 200', () => {
    expect(GITHUB_STRUCTURE_MAX_ENTRIES_PER_PAGE).toBe(200);
  });
});

describe('GitHub search: page-based pagination, exact fields', () => {
  it('defaults page to 1 when omitted', () => {
    const parsed = GitHubReposSearchBulkQueryLocalSchema.parse({
      queries: [{ keywords: ['x'] }],
    });
    const q = parsed.queries[0] as Record<string, unknown>;
    expect(q.page).toBe(1);
  });

  it('accepts explicit page > 1', () => {
    const parsed = GitHubReposSearchBulkQueryLocalSchema.parse({
      queries: [{ keywords: ['x'], page: 3 }],
    });
    const q = parsed.queries[0] as Record<string, unknown>;
    expect(q.page).toBe(3);
  });

  it('exposes limit but not githubAPILimit; limit undefined when not provided', () => {
    const parsed = GitHubCodeSearchBulkQueryLocalSchema.parse({
      queries: [{ keywords: ['x'] }],
    });
    const q = parsed.queries[0] as Record<string, unknown>;
    expect('githubAPILimit' in q).toBe(false);
    expect(q.limit).toBeUndefined();
  });
});

describe('Local tools: page-based pagination, exact fields', () => {
  const q0 = (
    schema: { parse: (v: unknown) => { queries: unknown[] } },
    query: unknown
  ) => schema.parse({ queries: [query] }).queries[0] as Record<string, unknown>;

  it('ripgrep: accepts page + itemsPerPage; removed filesPerPage/matchesPerPage stay absent', () => {
    const q = q0(LocalRipgrepBulkQuerySchema, {
      keywords: 'x',
      path: '.',
      page: 2,
      itemsPerPage: 10,
    });
    expect(q.page).toBe(2);
    expect(q.itemsPerPage).toBe(10);
    expect('filesPerPage' in q).toBe(false);
    expect('matchesPerPage' in q).toBe(false);
    expect('filePageNumber' in q).toBe(false);
  });

  it('localFindFiles: accepts page + itemsPerPage; removed filesPerPage stays absent', () => {
    const q = q0(LocalFindFilesBulkQuerySchema, {
      path: '.',
      names: ['*.ts'],
      page: 2,
      itemsPerPage: 25,
    });
    expect(q.page).toBe(2);
    expect(q.itemsPerPage).toBe(25);
    expect('filesPerPage' in q).toBe(false);
  });

  it('localViewStructure: accepts page + itemsPerPage; removed entriesPerPage stays absent', () => {
    const q = q0(LocalViewStructureBulkQuerySchema, {
      path: '.',
      page: 2,
      itemsPerPage: 30,
    });
    expect(q.page).toBe(2);
    expect(q.itemsPerPage).toBe(30);
    expect('entriesPerPage' in q).toBe(false);
  });

  it('ghViewRepoStructure: accepts page + itemsPerPage; removed entriesPerPage stays absent', () => {
    const q = q0(GitHubViewRepoStructureBulkQueryLocalSchema, {
      owner: 'o',
      repo: 'r',
      page: 2,
      itemsPerPage: 50,
    });
    expect(q.page).toBe(2);
    expect(q.itemsPerPage).toBe(50);
    expect('entriesPerPage' in q).toBe(false);
  });

  it('npmSearch: accepts page; removed page-size fields stay absent', () => {
    const q = q0(NpmSearchBulkQueryLocalSchema, {
      packageName: 'hono',
      page: 1,
    });
    expect(q.page).toBe(1);
    expect('itemsPerPage' in q).toBe(false);
    expect('searchLimit' in q).toBe(false);
  });
});
