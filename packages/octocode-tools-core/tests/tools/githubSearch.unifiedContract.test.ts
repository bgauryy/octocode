import { describe, expect, it } from 'vitest';

import { findDirectToolDefinition } from '../../src/tools/directToolCatalog.js';
import { ALL_TOOLS } from '../../src/tools/toolConfig.js';
import { buildGitHubSearchFinalizer } from '../../src/tools/github_search/finalizer.js';

const LEGACY_GITHUB_DISCOVERY_TOOLS = [
  'github.code',
  'github.repositories',
  'github.tree',
] as const;

describe('ghSearch unified public contract', () => {
  it('publishes strict code, repositories, and tree branches', () => {
    const definition = findDirectToolDefinition('ghSearch');
    expect(definition).toBeDefined();

    for (const query of [
      {
        operation: 'code',
        keywords: ['toolSchemas'],
        owner: 'bgauryy',
        repo: 'octocode',
      },
      {
        operation: 'repositories',
        keywords: ['octocode'],
        stars: '>100',
      },
      {
        operation: 'tree',
        owner: 'bgauryy',
        repo: 'octocode',
        path: 'packages',
        maxDepth: 2,
      },
    ]) {
      expect(
        definition!.schema.safeParse(query).success,
        JSON.stringify(query)
      ).toBe(true);
    }

    expect(
      definition!.schema.safeParse({
        operation: 'code',
        keywords: ['toolSchemas'],
        stars: '>100',
      }).success
    ).toBe(false);
    expect(
      definition!.schema.safeParse({
        operation: 'tree',
        owner: 'bgauryy',
        repo: 'octocode',
        keywords: ['toolSchemas'],
      }).success
    ).toBe(false);
  });

  it('makes ghSearch the only default code/repository/tree discovery tool', () => {
    expect(ALL_TOOLS.find(tool => tool.name === 'ghSearch')).toMatchObject({
      isDefault: true,
      isLocal: false,
      type: 'search',
    });
    for (const legacyName of LEGACY_GITHUB_DISCOVERY_TOOLS) {
      expect(ALL_TOOLS.some(tool => tool.name === legacyName)).toBe(false);
    }
  });

  it('rewrites legacy search continuations to the unified strict branch', () => {
    const output = buildGitHubSearchFinalizer()({
      queries: [{ operation: 'repositories', keywords: ['needle'] }],
      results: [
        {
          index: 0,
          status: 'success',
          data: {
            repositories: [{ fullPath: 'octocode/fixture' }],
            pagination: {
              currentPage: 1,
              totalPages: 2,
              hasMore: true,
              nextPage: 2,
            },
            next: {
              more: {
                tool: 'github.repositories',
                query: { keywords: ['needle'], page: 2 },
              },
            },
          },
        },
      ],
      config: { toolName: 'ghSearch' },
    });
    const serialized = JSON.stringify(output.structuredContent);
    expect(serialized).toContain('"tool":"ghSearch"');
    expect(serialized).toContain('"operation":"repositories"');
    expect(serialized).not.toContain('"tool":"github.repositories"');
  });

  it('declares GitHub search-window loss and suppresses a continuation beyond the provider cap', () => {
    const output = buildGitHubSearchFinalizer()({
      queries: [
        {
          operation: 'repositories',
          keywords: ['needle'],
          page: 500,
          pageSize: 2,
        },
      ],
      results: [
        {
          index: 0,
          status: 'success',
          data: {
            repositories: [{ fullPath: 'octocode/fixture' }],
            pagination: {
              currentPage: 500,
              perPage: 2,
              totalMatches: 1000,
              totalMatchesCapped: true,
              hasMore: true,
              nextPage: 501,
            },
          },
        },
      ],
      config: { toolName: 'ghSearch' },
    });
    const row = (
      output.structuredContent as {
        results: Array<{ data: Record<string, unknown> }>;
      }
    ).results[0]!.data as {
      isPartial?: boolean;
      terminalLimit?: boolean;
      partialReasons?: string[];
      pagination: Record<string, unknown>;
      next?: Record<string, unknown>;
    };

    expect(row).toMatchObject({
      isPartial: true,
      terminalLimit: true,
      partialReasons: ['providerResultCap'],
      pagination: {
        totalMatchesCapped: true,
        continuationUnavailable: { reason: 'providerResultCap' },
      },
    });
    expect(row.pagination.nextPage).toBeUndefined();
    expect(row.next?.nextPage).toBeUndefined();
  });
});
