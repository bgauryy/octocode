import { describe, it, expect } from 'vitest';
import { GitHubSearchBulkQuerySchema } from '../../../octocode-tools-core/src/tools/github_search/scheme.js';
import { LocalSearchBulkQuerySchema } from '../../../octocode-tools-core/src/tools/local_search/scheme.js';
import { LocalAnalyzeGraphBulkQuerySchema } from '../../../octocode-tools-core/src/tools/local_analyze_graph/scheme.js';
import { NpmSearchBulkQueryLocalSchema } from '../../../octocode-tools-core/src/tools/package_search/scheme.js';

const q0 = (
  schema: { parse: (value: unknown) => { queries: unknown[] } },
  query: unknown
) => schema.parse({ queries: [query] }).queries[0] as Record<string, unknown>;

describe('Unified public pagination fields', () => {
  it('ghSearch uses pageSize per page and does not expose a total limit', () => {
    const query = q0(GitHubSearchBulkQuerySchema, {
      operation: 'repositories',
      keywords: ['x'],
      page: 3,
      pageSize: 25,
    });
    expect(query).toMatchObject({ page: 3, pageSize: 25 });
    expect(
      GitHubSearchBulkQuerySchema.safeParse({
        queries: [{ operation: 'repositories', limit: 10 }],
      }).success
    ).toBe(false);
    expect(
      GitHubSearchBulkQuerySchema.safeParse({
        queries: [{ operation: 'repositories', itemsPerPage: 10 }],
      }).success
    ).toBe(false);
  });

  it('localSearch files uses limit as the total cap and pageSize per page', () => {
    const query = q0(LocalSearchBulkQuerySchema, {
      operation: 'files',
      path: '.',
      names: ['*.ts'],
      limit: 75,
      page: 2,
      pageSize: 25,
    });
    expect(query).toMatchObject({ limit: 75, page: 2, pageSize: 25 });
    expect('itemsPerPage' in query).toBe(false);
  });

  it('localSearch text uses maxFiles as its total cap, not limit', () => {
    const query = q0(LocalSearchBulkQuerySchema, {
      operation: 'text',
      path: '.',
      searchText: 'needle',
      maxFiles: 40,
      page: 2,
      pageSize: 10,
    });
    expect(query).toMatchObject({ maxFiles: 40, page: 2, pageSize: 10 });
    expect(
      LocalSearchBulkQuerySchema.safeParse({
        queries: [
          { operation: 'text', path: '.', searchText: 'needle', limit: 40 },
        ],
      }).success
    ).toBe(false);
  });

  it('localAnalyzeGraph distinguishes limit from pageSize', () => {
    const query = q0(LocalAnalyzeGraphBulkQuerySchema, {
      operation: 'cycles',
      path: '.',
      limit: 100,
      page: 2,
      pageSize: 20,
    });
    expect(query).toMatchObject({ limit: 100, page: 2, pageSize: 20 });
  });

  it('npmSearch exposes page and pageSize only for keyword discovery', () => {
    const keywordQuery = q0(NpmSearchBulkQueryLocalSchema, {
      keywords: ['hono'],
      page: 2,
      pageSize: 25,
    });
    expect(keywordQuery).toMatchObject({ page: 2, pageSize: 25 });
    for (const field of ['itemsPerPage', 'searchLimit', 'limit']) {
      expect(field in keywordQuery).toBe(false);
    }

    const exactQuery = q0(NpmSearchBulkQueryLocalSchema, {
      packageName: 'hono',
    });
    for (const field of [
      'page',
      'pageSize',
      'itemsPerPage',
      'searchLimit',
      'limit',
    ]) {
      expect(field in exactQuery).toBe(false);
    }
  });
});
