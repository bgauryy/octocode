import { describe, it, expect } from 'vitest';
import { GitHubSearchBulkQuerySchema } from '@octocodeai/config/schema';
import { LocalSearchBulkQuerySchema } from '@octocodeai/config/schema';
import { AstSearchBulkQuerySchema } from '@octocodeai/config/schema';
import { ArtifactSearchBulkQueryLocalSchema } from '@octocodeai/config/schema';

const q0 = (
  schema: { parse: (value: unknown) => { queries: unknown[] } },
  query: unknown
) => schema.parse({ queries: [query] }).queries[0] as Record<string, unknown>;

describe('Unified public pagination fields', () => {
  it('ghSearch uses pageSize per page and does not expose a total limit', () => {
    const query = q0(GitHubSearchBulkQuerySchema, {
      reasoning: 'exercise pagination fields',
      operation: 'repositories',
      keywords: ['x'],
      page: 3,
      pageSize: 25,
    });
    expect(query).toMatchObject({ page: 3, pageSize: 25 });
    expect(
      GitHubSearchBulkQuerySchema.safeParse({
        queries: [
          {
            reasoning: 'exercise pagination fields',
            operation: 'repositories',
            limit: 10,
          },
        ],
      }).success
    ).toBe(false);
    expect(
      GitHubSearchBulkQuerySchema.safeParse({
        queries: [
          {
            reasoning: 'exercise pagination fields',
            operation: 'repositories',
            itemsPerPage: 10,
          },
        ],
      }).success
    ).toBe(false);
  });

  it('astSearch files uses limit as the total cap and pageSize per page', () => {
    const query = q0(AstSearchBulkQuerySchema, {
      reasoning: 'exercise pagination fields',
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
      reasoning: 'exercise pagination fields',
      path: '.',
      searchText: 'needle',
      regex: 'literal',
      maxFiles: 40,
      page: 2,
      pageSize: 10,
    });
    expect(query).toMatchObject({ maxFiles: 40, page: 2, pageSize: 10 });
    expect(
      LocalSearchBulkQuerySchema.safeParse({
        queries: [
          {
            reasoning: 'exercise pagination fields',
            path: '.',
            searchText: 'needle',
            regex: 'literal',
            limit: 40,
          },
        ],
      }).success
    ).toBe(false);
  });

  it('astSearch topology distinguishes limit from pageSize', () => {
    const query = q0(AstSearchBulkQuerySchema, {
      reasoning: 'exercise pagination fields',
      operation: 'topology',
      analysis: 'cycles',
      path: '.',
      limit: 100,
      page: 2,
      pageSize: 20,
    });
    expect(query).toMatchObject({ limit: 100, page: 2, pageSize: 20 });
  });

  it('artifactSearch exposes cursor and pageSize only for keyword discovery', () => {
    const keywordQuery = q0(ArtifactSearchBulkQueryLocalSchema, {
      reasoning: 'exercise pagination fields',
      type: 'npm',
      keywords: ['hono'],
      cursor: 'opaque',
      pageSize: 25,
    });
    expect(keywordQuery).toMatchObject({ cursor: 'opaque', pageSize: 25 });
    for (const field of ['itemsPerPage', 'searchLimit', 'limit']) {
      expect(field in keywordQuery).toBe(false);
    }

    const exactQuery = q0(ArtifactSearchBulkQueryLocalSchema, {
      reasoning: 'exercise pagination fields',
      type: 'npm',
      packageName: 'hono',
    });
    for (const field of [
      'page',
      'cursor',
      'pageSize',
      'itemsPerPage',
      'searchLimit',
      'limit',
    ]) {
      expect(field in exactQuery).toBe(false);
    }
  });
});
