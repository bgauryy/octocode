import { describe, it, expect } from 'vitest';
import { GitHubSearchRepoBulkQuerySchema } from '@octocodeai/config/schema';
import { LocalSearchBulkQuerySchema } from '@octocodeai/config/schema';
import {
  AstTopologyBulkQuerySchema,
  StructureSearchBulkQuerySchema,
} from '@octocodeai/config/schema';
import { ArtifactSearchBulkQueryLocalSchema } from '@octocodeai/config/schema';

const q0 = (
  schema: { parse: (value: unknown) => { queries: unknown[] } },
  query: unknown
) => schema.parse({ queries: [query] }).queries[0] as Record<string, unknown>;

describe('Unified public pagination fields', () => {
  it('ghSearchRepo uses pageSize per page and does not expose a total limit', () => {
    const query = q0(GitHubSearchRepoBulkQuerySchema, {
      mainGoal: 'test goal',
      reasoning: 'exercise pagination fields',
      keywords: ['x'],
      page: 3,
      pageSize: 25,
    });
    expect(query).toMatchObject({ page: 3, pageSize: 25 });
    expect(
      GitHubSearchRepoBulkQuerySchema.safeParse({
        queries: [
          {
            mainGoal: 'test goal',
            reasoning: 'exercise pagination fields',
            limit: 10,
          },
        ],
      }).success
    ).toBe(false);
    expect(
      GitHubSearchRepoBulkQuerySchema.safeParse({
        queries: [
          {
            mainGoal: 'test goal',
            reasoning: 'exercise pagination fields',
            itemsPerPage: 10,
          },
        ],
      }).success
    ).toBe(false);
  });

  it('structureSearch files uses limit as the total cap and pageSize per page', () => {
    const query = q0(StructureSearchBulkQuerySchema, {
      mainGoal: 'test goal',
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

  it('localSearch text uses pageSize without redundant cap aliases', () => {
    const base = {
      mainGoal: 'test goal',
      reasoning: 'exercise pagination fields',
      path: '.',
      searchText: 'needle',
      regex: 'literal',
    };
    const query = q0(LocalSearchBulkQuerySchema, {
      ...base,
      page: 2,
      pageSize: 10,
    });
    expect(query).toMatchObject({ page: 2, pageSize: 10 });
    for (const retired of [{ maxFiles: 40 }, { limit: 40 }]) {
      expect(
        LocalSearchBulkQuerySchema.safeParse({
          queries: [{ ...base, ...retired }],
        }).success
      ).toBe(false);
    }
  });

  it('astTopology distinguishes limit from pageSize', () => {
    const query = q0(AstTopologyBulkQuerySchema, {
      mainGoal: 'test goal',
      reasoning: 'exercise pagination fields',
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
      mainGoal: 'test goal',
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
      mainGoal: 'test goal',
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
