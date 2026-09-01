import { describe, expect, it } from 'vitest';
import { z } from 'zod';

import { PUBLIC_TOOL_DESCRIPTIONS } from '../../src/toolContract/descriptions.js';
import { DIRECT_TOOL_SPECIFICATIONS } from '../../src/tools/directToolCatalog/toolSpecifications.js';
import {
  GitHubGetHistoryItemBulkQueryLocalSchema,
  GitHubGetHistoryItemQueryLocalSchema,
  GitHubSearchHistoryBulkQueryLocalSchema,
  GitHubSearchHistoryQueryLocalSchema,
} from '../../src/tools/github_search_pull_requests/historySchemes.js';
import {
  ALL_TOOLS,
  GITHUB_GET_HISTORY_ITEM,
  GITHUB_SEARCH_HISTORY,
} from '../../src/tools/toolConfig.js';

function expectSchemaResult(
  schema: z.ZodType,
  query: unknown,
  expected: boolean
): void {
  const generated = z.fromJSONSchema(z.toJSONSchema(schema, { io: 'input' }));
  expect(schema.safeParse(query).success).toBe(expected);
  expect(generated.safeParse(query).success).toBe(expected);
}

describe('two-tool GitHub history public contract', () => {
  it('hard-cuts the three legacy public tools from catalog and specifications', () => {
    const expected = ['ghSearchHistory', 'ghGetHistoryItem'];
    const retired = [
      'ghSearchPullRequests',
      'ghSearchIssues',
      'ghSearchCommits',
    ];
    const configuredNames = ALL_TOOLS.map(tool => tool.name);
    const specificationNames = DIRECT_TOOL_SPECIFICATIONS.map(
      tool => tool.name
    );

    expect(configuredNames).toEqual(expect.arrayContaining(expected));
    expect(specificationNames).toEqual(expect.arrayContaining(expected));
    expect(configuredNames).not.toEqual(expect.arrayContaining(retired));
    expect(specificationNames).not.toEqual(expect.arrayContaining(retired));
    expect(GITHUB_SEARCH_HISTORY.name).toBe('ghSearchHistory');
    expect(GITHUB_GET_HISTORY_ITEM.name).toBe('ghGetHistoryItem');
    expect(PUBLIC_TOOL_DESCRIPTIONS).toHaveProperty('ghSearchHistory');
    expect(PUBLIC_TOOL_DESCRIPTIONS).toHaveProperty('ghGetHistoryItem');
    for (const name of retired) {
      expect(PUBLIC_TOOL_DESCRIPTIONS).not.toHaveProperty(name);
    }
  });

  it.each([
    {
      operation: 'pullRequests',
      owner: 'octo',
      repo: 'repo',
      keywords: ['schema'],
      state: 'open',
      page: 2,
    },
    {
      operation: 'issues',
      owner: 'octo',
      repo: 'repo',
      keywords: ['bug'],
      label: ['regression'],
      page: 2,
    },
    {
      operation: 'commits',
      owner: 'octo',
      repo: 'repo',
      branch: 'main',
      since: '3m',
      page: 2,
    },
  ])('accepts the $operation search/list branch', query => {
    expectSchemaResult(GitHubSearchHistoryQueryLocalSchema, query, true);
  });

  it('preserves global pull-request discovery without repository scope', () => {
    expectSchemaResult(
      GitHubSearchHistoryQueryLocalSchema,
      {
        operation: 'pullRequests',
        keywords: ['strict schema'],
        state: 'open',
      },
      true
    );
  });

  it.each([
    {
      operation: 'pullRequest',
      owner: 'octo',
      repo: 'repo',
      number: 42,
      content: {
        body: true,
        changedFiles: true,
        patches: { mode: 'selected', files: ['src/index.ts'] },
        comments: { discussion: true },
      },
      filePage: 2,
      commentPage: 3,
    },
    {
      operation: 'issue',
      owner: 'octo',
      repo: 'repo',
      number: 7,
      content: { body: true, comments: { discussion: true } },
      commentPage: 2,
    },
    {
      operation: 'commit',
      owner: 'octo',
      repo: 'repo',
      ref: 'abc123',
      includeDiff: true,
      path: 'src/index.ts',
      filePage: 2,
      charOffset: 10,
    },
    {
      operation: 'compare',
      owner: 'octo',
      repo: 'repo',
      base: 'main',
      head: 'feature',
      includeDiff: true,
      filePage: 2,
    },
  ])('accepts the $operation exact-retrieval branch', query => {
    expectSchemaResult(GitHubGetHistoryItemQueryLocalSchema, query, true);
  });

  it('rejects cross-branch fields and any generic cursor', () => {
    const invalidSearchQueries = [
      { operation: 'pullRequests', owner: 'o', repo: 'r', number: 1 },
      {
        operation: 'pullRequests',
        owner: 'o',
        repo: 'r',
        content: { body: true },
      },
      {
        operation: 'issues',
        owner: 'o',
        repo: 'r',
        number: 1,
      },
      {
        operation: 'issues',
        owner: 'o',
        repo: 'r',
        commentPage: 2,
      },
      { operation: 'issues', owner: 'o', repo: 'r', branch: 'main' },
      { operation: 'commits', owner: 'o', repo: 'r', state: 'open' },
      {
        operation: 'commits',
        owner: 'o',
        repo: 'r',
        includeDiff: true,
      },
      { operation: 'commits', owner: 'o', repo: 'r', filePage: 2 },
      { operation: 'commits', owner: 'o', repo: 'r', charOffset: 10 },
      { operation: 'commits', owner: 'o', repo: 'r', charLength: 10 },
      { operation: 'commits', owner: 'o', repo: 'r', ref: 'abc' },
      { operation: 'commits', owner: 'o', repo: 'r', base: 'main' },
      { operation: 'commits', owner: 'o', repo: 'r', head: 'next' },
      { operation: 'commits', owner: 'o', repo: 'r', cursor: 'opaque' },
      {
        operation: 'pullRequests',
        keywordsToSearch: ['legacy'],
      },
      {
        operation: 'pullRequests',
        itemsPerPage: 10,
      },
    ];
    const invalidItemQueries = [
      { operation: 'pullRequest', owner: 'o', repo: 'r' },
      { operation: 'issue', owner: 'o', repo: 'r' },
      { operation: 'commit', owner: 'o', repo: 'r' },
      { operation: 'compare', owner: 'o', repo: 'r', base: 'main' },
      { operation: 'compare', owner: 'o', repo: 'r', head: 'next' },
      {
        operation: 'pullRequest',
        owner: 'o',
        repo: 'r',
        number: 1,
        ref: 'abc',
      },
      {
        operation: 'pullRequest',
        owner: 'o',
        repo: 'r',
        prNumber: 1,
      },
      {
        operation: 'issue',
        owner: 'o',
        repo: 'r',
        issueNumber: 1,
      },
      {
        operation: 'pullRequest',
        owner: 'o',
        repo: 'r',
        number: 1,
        keywords: ['wrong'],
      },
      {
        operation: 'pullRequest',
        owner: 'o',
        repo: 'r',
        number: 1,
        state: 'open',
      },
      {
        operation: 'pullRequest',
        owner: 'o',
        repo: 'r',
        number: 1,
        page: 2,
      },
      {
        operation: 'issue',
        owner: 'o',
        repo: 'r',
        number: 1,
        filePage: 2,
      },
      {
        operation: 'issue',
        owner: 'o',
        repo: 'r',
        number: 1,
        commitPage: 2,
      },
      {
        operation: 'issue',
        owner: 'o',
        repo: 'r',
        number: 1,
        content: { reviews: true },
      },
      {
        operation: 'commit',
        owner: 'o',
        repo: 'r',
        ref: 'abc',
        number: 1,
      },
      {
        operation: 'commit',
        owner: 'o',
        repo: 'r',
        ref: 'abc',
        base: 'main',
        head: 'next',
      },
      {
        operation: 'commit',
        owner: 'o',
        repo: 'r',
        ref: 'abc',
        branch: 'main',
      },
      {
        operation: 'commit',
        owner: 'o',
        repo: 'r',
        ref: 'abc',
        since: '3m',
      },
      {
        operation: 'commit',
        owner: 'o',
        repo: 'r',
        ref: 'abc',
        page: 2,
      },
      {
        operation: 'compare',
        owner: 'o',
        repo: 'r',
        base: 'main',
        head: 'next',
        ref: 'abc',
      },
      {
        operation: 'compare',
        owner: 'o',
        repo: 'r',
        base: 'main',
        head: 'next',
        cursor: 'opaque',
      },
      {
        operation: 'compare',
        owner: 'o',
        repo: 'r',
        base: 'main',
        head: 'next',
        number: 1,
      },
      {
        operation: 'compare',
        owner: 'o',
        repo: 'r',
        base: 'main',
        head: 'next',
        branch: 'main',
      },
      {
        operation: 'compare',
        owner: 'o',
        repo: 'r',
        base: 'main',
        head: 'next',
        since: '3m',
      },
      {
        operation: 'compare',
        owner: 'o',
        repo: 'r',
        base: 'main',
        head: 'next',
        page: 2,
      },
      {
        operation: 'compare',
        owner: 'o',
        repo: 'r',
        base: 'main',
        head: 'next',
        commentPage: 2,
      },
      {
        operation: 'commit',
        owner: 'o',
        repo: 'r',
        ref: 'abc',
        filePath: 'legacy.ts',
      },
    ];

    for (const query of invalidSearchQueries) {
      expectSchemaResult(GitHubSearchHistoryQueryLocalSchema, query, false);
    }
    for (const query of invalidItemQueries) {
      expectSchemaResult(GitHubGetHistoryItemQueryLocalSchema, query, false);
    }
  });

  it('applies the same strict discriminated contract to bulk inputs', () => {
    expectSchemaResult(
      GitHubSearchHistoryBulkQueryLocalSchema,
      {
        queries: [
          { operation: 'pullRequests', owner: 'o', repo: 'r' },
          { operation: 'commits', owner: 'o', repo: 'r', page: 2 },
        ],
      },
      true
    );
    expectSchemaResult(
      GitHubSearchHistoryBulkQueryLocalSchema,
      {
        queries: [
          { operation: 'pullRequests', owner: 'o', repo: 'r', ref: 'abc' },
        ],
      },
      false
    );
    expectSchemaResult(
      GitHubGetHistoryItemBulkQueryLocalSchema,
      {
        queries: [
          { operation: 'issue', owner: 'o', repo: 'r', number: 9 },
          {
            operation: 'compare',
            owner: 'o',
            repo: 'r',
            base: 'main',
            head: 'next',
          },
        ],
      },
      true
    );
    expectSchemaResult(
      GitHubGetHistoryItemBulkQueryLocalSchema,
      {
        queries: [
          {
            operation: 'issue',
            owner: 'o',
            repo: 'r',
            number: 9,
            branch: 'main',
          },
        ],
      },
      false
    );
  });
});
