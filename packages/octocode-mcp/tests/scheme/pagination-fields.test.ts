import { describe, it, expect, assert } from 'vitest';
import { GitHubSearchRepoBulkQuerySchema } from '@octocodeai/config/schema';
import { LocalSearchBulkQuerySchema } from '@octocodeai/config/schema';
import { StructureSearchBulkQuerySchema } from '@octocodeai/config/schema';
import { ArtifactSearchBulkQueryLocalSchema } from '@octocodeai/config/schema';

const q0 = (
  schema: { parse: (value: unknown) => { queries?: unknown[] | undefined } },
  query: unknown
) => {
  const parsed = schema.parse({ queries: [query] });
  assert(parsed.queries, 'Fresh input retains its query batch');
  return parsed.queries[0] as Record<string, unknown>;
};

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

  it('structureSearch files uses maxEntries as the scan cap and pageSize per page', () => {
    const query = q0(StructureSearchBulkQuerySchema, {
      mainGoal: 'test goal',
      reasoning: 'exercise pagination fields',
      operation: 'files',
      path: '.',
      include: ['*.ts'],
      maxEntries: 75,
      page: 2,
      pageSize: 25,
    });
    expect(query).toMatchObject({ maxEntries: 75, page: 2, pageSize: 25 });
    expect('itemsPerPage' in query).toBe(false);
  });

  it('localSearch text uses pageSize without redundant cap aliases', () => {
    const base = {
      mainGoal: 'test goal',
      reasoning: 'exercise pagination fields',
      path: '.',
      matchString: 'needle',
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

  it('artifactSearch exposes page and pageSize only for keyword discovery', () => {
    const keywordQuery = q0(ArtifactSearchBulkQueryLocalSchema, {
      mainGoal: 'test goal',
      reasoning: 'exercise pagination fields',
      ecosystem: 'npm',
      keywords: ['hono'],
      page: 2,
      pageSize: 25,
    });
    expect(keywordQuery).toMatchObject({ page: 2, pageSize: 25 });
    for (const field of ['itemsPerPage', 'searchLimit', 'limit']) {
      expect(field in keywordQuery).toBe(false);
    }

    const exactQuery = q0(ArtifactSearchBulkQueryLocalSchema, {
      mainGoal: 'test goal',
      reasoning: 'exercise pagination fields',
      ecosystem: 'npm',
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
