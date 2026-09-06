import type { CodeSearchResult } from '../../providers/providerResults.js';
import type { z } from 'zod';
import type { GitHubCodeSearchQuerySchema } from '../../toolContract/input/resources/tools/githubCodeOperation.js';
import type { WithOptionalMeta } from '../../types/execution.js';

import {
  countMetadata,
  splitRepositoryPath,
  toProviderProjectId,
} from './shared.js';

type GitHubCodeSearchQuery = z.infer<typeof GitHubCodeSearchQuerySchema>;

export function mapCodeSearchToolQuery(
  query: WithOptionalMeta<GitHubCodeSearchQuery>
) {
  return {
    keywords: query.keywords ?? [],
    projectId: toProviderProjectId(query.owner, query.repo),
    owner: query.owner,
    path: query.path,
    filename: query.filename,
    extension: query.extension,
    language: (query as Record<string, unknown>).language as string | undefined,
    match: query.match,
    limit: (query as Record<string, unknown>).limit as number | undefined,
    page: query.page,
    goal: query.goal,
    reasoning: query.reasoning,
  };
}

export interface CodeSearchGroupedMatch {
  path: string;
  value?: string;

  pathOnly?: boolean;

  matchIndices?: Array<{ start: number; end: number; lineOffset: number }>;
}

export interface CodeSearchGroupedResult {
  id: string;
  queryIndex?: number;
  owner: string;
  repo: string;
  matches: CodeSearchGroupedMatch[];
}

export interface CodeSearchPagination {
  currentPage: number;
  totalPages: number;
  perPage: number;
  totalMatches: number;
  reportedTotalMatches?: number;
  reachableTotalMatches?: number;
  totalMatchesKind?: 'exact' | 'reported' | 'lowerBound';
  totalMatchesCapped?: boolean;
  hasMore: boolean;
  nextPage?: number;
  uniqueFileCount?: number;
}

export interface CodeSearchFlatResult {
  results: CodeSearchGroupedResult[];
  pagination?: CodeSearchPagination;

  nonExistentScope?: boolean;

  incompleteResults?: boolean;
}

export function mapCodeSearchProviderResult(
  data: CodeSearchResult,
  query: WithOptionalMeta<GitHubCodeSearchQuery>
): CodeSearchFlatResult {
  const isPathMatch = query.match === 'path';
  const groups = new Map<string, CodeSearchGroupedResult>();

  for (const item of data.items) {
    const repoFullName = item.repository.name || '';
    const { owner, repo } = splitRepositoryPath(repoFullName);
    const id = `${owner}/${repo}`;

    let group = groups.get(id);
    if (!group) {
      group = { id, owner, repo, matches: [] };
      groups.set(id, group);
    }

    if (isPathMatch || !item.matches?.length) {
      group.matches.push({
        path: item.path,
        ...(!isPathMatch ? { pathOnly: true } : {}),
      });
      continue;
    }

    let emittedMatchForItem = false;
    for (const m of item.matches) {
      if (!m.context) continue;
      // Shared response pagination bounds output without losing fragment tails.
      const value = m.context;
      const match: CodeSearchGroupedMatch = {
        path: item.path,
        value,
      };
      if (m.positions?.length > 0) {
        const inRange = m.positions.filter(
          ([start, end]) => start >= 0 && end > start && end <= value.length
        );
        if (inRange.length > 0) {
          match.matchIndices = inRange.map(([start, end]) => ({
            start,
            end,
            lineOffset:
              (m.context ?? '').substring(0, start).split('\n').length - 1,
          }));
        }
      }
      group.matches.push(match);
      emittedMatchForItem = true;
    }

    if (!emittedMatchForItem) {
      group.matches.push({
        path: item.path,
        pathOnly: true,
      });
    }
  }

  const result: CodeSearchFlatResult = {
    results: Array.from(groups.values()),
    ...(data.nonExistentScope ? { nonExistentScope: true } : {}),
    ...(data.incompleteResults ? { incompleteResults: true } : {}),
  };

  if (data.pagination && data.pagination.totalPages > 1) {
    result.pagination = {
      currentPage: data.pagination.currentPage,
      totalPages: data.pagination.totalPages,
      perPage: data.pagination.entriesPerPage || 10,
      totalMatches: data.pagination.totalMatches || 0,
      ...countMetadata(data.pagination),
      hasMore: data.pagination.hasMore,
      ...(data.pagination.hasMore
        ? { nextPage: data.pagination.currentPage + 1 }
        : {}),
    };
  }

  return result;
}
