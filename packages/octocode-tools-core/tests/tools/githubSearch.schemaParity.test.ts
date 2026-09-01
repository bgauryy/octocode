import { describe, expect, it } from 'vitest';
import { z } from 'zod';
import {
  GITHUB_SEARCH_DEFAULT_LIMIT,
  GITHUB_SEARCH_MAX_LIMIT,
  GITHUB_STRUCTURE_DEFAULT_ENTRIES_PER_PAGE,
  GITHUB_STRUCTURE_MAX_ENTRIES_PER_PAGE,
} from '../../src/config.js';
import { buildRepoSearchQuery } from '../../src/github/queryBuilders.js';
import { GitHubSearchQuerySchema } from '../../src/tools/github_search/scheme.js';
import { GitHubCodeSearchQueryLocalSchema } from '../../src/tools/github_search_code/scheme.js';
import { GitHubReposSearchSingleQueryLocalSchema } from '../../src/tools/github_search_repos/scheme.js';
import { GitHubViewRepoStructureQueryLocalSchema } from '../../src/tools/github_view_repo_structure/scheme.js';
import { mapRepoSearchToolQuery } from '../../src/tools/providerMappers/repoSearch.js';
import { hasValidRepositorySearchParams } from '../../src/tools/github_search_repos/execution/queryVariants.js';

type JsonObjectSchema = {
  oneOf?: JsonObjectSchema[];
  properties?: Record<string, { const?: unknown }>;
};

function jsonProperties(schema: z.ZodType): string[] {
  const json = z.toJSONSchema(schema, { io: 'input' }) as JsonObjectSchema;
  return Object.keys(json.properties ?? {}).sort();
}

function unifiedBranchProperties(operation: string): string[] {
  const json = z.toJSONSchema(GitHubSearchQuerySchema, {
    io: 'input',
  }) as JsonObjectSchema;
  const branch = json.oneOf?.find(
    candidate => candidate.properties?.operation?.const === operation
  );
  expect(branch, `missing unified ${operation} branch`).toBeDefined();
  return Object.keys(branch?.properties ?? {})
    .filter(field => field !== 'operation')
    .sort();
}

describe('ghSearch schema parity with legacy GitHub discovery tools', () => {
  it.each(['code', 'repositories'] as const)(
    'rejects an operation-only %s query at the public schema boundary',
    operation => {
      expect(GitHubSearchQuerySchema.safeParse({ operation }).success).toBe(
        false
      );
    }
  );

  it.each([
    ['code', GitHubCodeSearchQueryLocalSchema],
    ['repositories', GitHubReposSearchSingleQueryLocalSchema],
    ['tree', GitHubViewRepoStructureQueryLocalSchema],
  ] as const)(
    '%s exposes one canonical field per legacy capability',
    (operation, legacy) => {
      const canonicalLegacyFields = jsonProperties(legacy)
        .map(field => {
          if (operation === 'repositories' && field === 'topicsToSearch')
            return 'topics';
          if (field === 'limit' || field === 'itemsPerPage') return 'pageSize';
          return field;
        })
        .sort();
      expect(unifiedBranchProperties(operation)).toEqual(canonicalLegacyFields);
    }
  );

  it('keeps defaults equivalent at the provider boundary', () => {
    const unifiedCode = GitHubSearchQuerySchema.parse({
      operation: 'code',
      keywords: ['needle'],
    });
    const legacyCode = GitHubCodeSearchQueryLocalSchema.parse({
      keywords: ['needle'],
    });
    expect(unifiedCode).toMatchObject({
      match: 'file',
      pageSize: GITHUB_SEARCH_DEFAULT_LIMIT,
      page: 1,
    });
    expect(legacyCode).toMatchObject({ match: 'file', page: 1 });
    expect(legacyCode.limit ?? GITHUB_SEARCH_DEFAULT_LIMIT).toBe(
      unifiedCode.pageSize
    );

    const unifiedRepos = GitHubSearchQuerySchema.parse({
      operation: 'repositories',
      keywords: ['needle'],
    });
    const legacyRepos = GitHubReposSearchSingleQueryLocalSchema.parse({
      keywords: ['needle'],
    });
    expect(unifiedRepos).toMatchObject({
      sort: 'best-match',
      pageSize: GITHUB_SEARCH_DEFAULT_LIMIT,
      page: 1,
    });
    expect(legacyRepos).toMatchObject({ sort: 'best-match', page: 1 });
    expect(legacyRepos.limit ?? GITHUB_SEARCH_DEFAULT_LIMIT).toBe(
      unifiedRepos.pageSize
    );

    const unifiedTree = GitHubSearchQuerySchema.parse({
      operation: 'tree',
      owner: 'octocode-ai',
      repo: 'octocode',
    });
    const legacyTree = GitHubViewRepoStructureQueryLocalSchema.parse({
      owner: 'octocode-ai',
      repo: 'octocode',
    });
    expect(unifiedTree).toMatchObject({
      pageSize: GITHUB_STRUCTURE_DEFAULT_ENTRIES_PER_PAGE,
      page: 1,
    });
    expect(legacyTree).toMatchObject({ page: 1 });
    expect(
      legacyTree.itemsPerPage ?? GITHUB_STRUCTURE_DEFAULT_ENTRIES_PER_PAGE
    ).toBe(unifiedTree.pageSize);
  });

  it('shares every numeric bound while retaining relaxed legacy clamping', () => {
    const cases = [
      {
        operation: 'code',
        legacy: GitHubCodeSearchQueryLocalSchema,
        input: { keywords: ['needle'] },
        field: 'pageSize',
        min: 1,
        max: GITHUB_SEARCH_MAX_LIMIT,
      },
      {
        operation: 'code',
        legacy: GitHubCodeSearchQueryLocalSchema,
        input: { keywords: ['needle'] },
        field: 'page',
        min: 1,
        max: 1000,
      },
      {
        operation: 'repositories',
        legacy: GitHubReposSearchSingleQueryLocalSchema,
        input: { keywords: ['needle'] },
        field: 'pageSize',
        min: 1,
        max: GITHUB_SEARCH_MAX_LIMIT,
      },
      {
        operation: 'repositories',
        legacy: GitHubReposSearchSingleQueryLocalSchema,
        input: { keywords: ['needle'] },
        field: 'page',
        min: 1,
        max: 1000,
      },
      {
        operation: 'tree',
        legacy: GitHubViewRepoStructureQueryLocalSchema,
        input: { owner: 'octocode-ai', repo: 'octocode' },
        field: 'maxDepth',
        min: 0,
        max: 20,
      },
      {
        operation: 'tree',
        legacy: GitHubViewRepoStructureQueryLocalSchema,
        input: { owner: 'octocode-ai', repo: 'octocode' },
        field: 'pageSize',
        min: 1,
        max: GITHUB_STRUCTURE_MAX_ENTRIES_PER_PAGE,
      },
      {
        operation: 'tree',
        legacy: GitHubViewRepoStructureQueryLocalSchema,
        input: { owner: 'octocode-ai', repo: 'octocode' },
        field: 'page',
        min: 1,
        max: 1000,
      },
    ] as const;

    for (const { operation, legacy, input, field, min, max } of cases) {
      const legacyField =
        field === 'pageSize'
          ? operation === 'tree'
            ? 'itemsPerPage'
            : 'limit'
          : field;
      for (const value of [min, max]) {
        expect(
          GitHubSearchQuerySchema.safeParse({
            operation,
            ...input,
            [field]: value,
          }).success,
          `${operation}.${field} accepts ${value}`
        ).toBe(true);
        expect(
          legacy.safeParse({ ...input, [legacyField]: value }).success,
          `legacy ${operation}.${legacyField} accepts ${value}`
        ).toBe(true);
      }

      expect(
        GitHubSearchQuerySchema.safeParse({
          operation,
          ...input,
          [field]: max + 1,
        }).success,
        `${operation}.${field} rejects ${max + 1}`
      ).toBe(false);
      expect(
        legacy.parse({ ...input, [legacyField]: max + 1 })[legacyField],
        `legacy ${operation}.${legacyField} clamps ${max + 1}`
      ).toBe(max);
    }
  });

  it('keeps the colliding match field branch-specific', () => {
    expect(
      GitHubSearchQuerySchema.safeParse({
        operation: 'code',
        keywords: ['needle'],
        match: 'path',
      }).success
    ).toBe(true);
    expect(
      GitHubSearchQuerySchema.safeParse({
        operation: 'repositories',
        keywords: ['needle'],
        match: ['name', 'readme'],
      }).success
    ).toBe(true);
    expect(
      GitHubSearchQuerySchema.safeParse({
        operation: 'code',
        keywords: ['needle'],
        match: ['name'],
      }).success
    ).toBe(false);
    expect(
      GitHubSearchQuerySchema.safeParse({
        operation: 'repositories',
        keywords: ['needle'],
        match: 'path',
      }).success
    ).toBe(false);
  });

  it('accepts the smallest runnable search constraint for each discovery branch', () => {
    expect(
      GitHubSearchQuerySchema.safeParse({
        operation: 'code',
        owner: 'octocode-ai',
      }).success
    ).toBe(true);
    expect(
      GitHubSearchQuerySchema.safeParse({
        operation: 'repositories',
        archived: false,
      }).success
    ).toBe(true);
  });
});

describe('repository provider parity', () => {
  const repositoryQuery = {
    keywords: ['needle'],
    forks: '>=10',
    goodFirstIssues: '>2',
    created: '>=2024-01-01',
    size: '100..2000',
  };

  it('forwards every legacy repository qualifier to providers', () => {
    expect(mapRepoSearchToolQuery(repositoryQuery)).toMatchObject(
      repositoryQuery
    );
  });

  it('renders every forwarded qualifier into the raw GitHub query', () => {
    expect(buildRepoSearchQuery(repositoryQuery)).toContain('forks:>=10');
    expect(buildRepoSearchQuery(repositoryQuery)).toContain(
      'good-first-issues:>2'
    );
    expect(buildRepoSearchQuery(repositoryQuery)).toContain(
      'created:>=2024-01-01'
    );
    expect(buildRepoSearchQuery(repositoryQuery)).toContain('size:100..2000');
  });

  it.each([
    ['created', '>=2024-01-01'],
    ['size', '100..2000'],
  ] as const)('accepts %s as the sole repository filter', (field, value) => {
    expect(hasValidRepositorySearchParams({ [field]: value })).toBe(true);
  });
});
