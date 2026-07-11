import type { CodeSearchResult } from '../../providers/types.js';
import type { z } from 'zod';
import type { GitHubCodeSearchQuerySchema } from '@octocodeai/octocode-core/schemas';
import type { WithOptionalMeta } from '../../types/execution.js';

import { DEFAULT_MATCH_SNIPPET_CHARS } from '../../config.js';
import {
  countMetadata,
  splitRepositoryPath,
  toProviderProjectId,
} from './shared.js';

type GitHubCodeSearchQuery = z.infer<typeof GitHubCodeSearchQuerySchema>;

/**
 * Char-boundary truncation mirroring the Rust engine's `truncate_unicode`:
 * keeps at most `maxChars` Unicode scalars and appends `...` when it cuts.
 * Never slices UTF-8 mid-codepoint or mid-token — the single data-layer bound
 * for GitHub code-search match fragments (the render layer must not re-trim).
 */
export function truncateSnippetChars(
  value: string,
  maxChars = DEFAULT_MATCH_SNIPPET_CHARS
): string {
  if (maxChars <= 0) return '';
  const chars = [...value];
  if (chars.length <= maxChars) return value;
  if (maxChars <= 3) return '.'.repeat(maxChars);
  return chars.slice(0, maxChars - 3).join('') + '...';
}

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
    mainResearchGoal: query.mainResearchGoal,
    researchGoal: query.researchGoal,
    reasoning: query.reasoning,
  };
}

export interface CodeSearchGroupedMatch {
  path: string;
  value?: string;

  pathOnly?: boolean;

  matchIndices?: Array<{ start: number; end: number; lineOffset: number }>;

  url?: string;
}

export interface CodeSearchGroupedResult {
  id: string;
  queryId?: string;
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
  const verbose = (query as { verbose?: boolean }).verbose === true;
  const groups = new Map<string, CodeSearchGroupedResult>();

  for (const item of data.items) {
    const repoFullName = item.repository.name || '';
    const { owner, repo } = splitRepositoryPath(repoFullName);
    const id = `${owner}/${repo}`;

    const itemExtra = item as { url?: string };
    let group = groups.get(id);
    if (!group) {
      group = { id, owner, repo, matches: [] };
      groups.set(id, group);
    }

    if (isPathMatch || !item.matches?.length) {
      group.matches.push({
        path: item.path,
        ...(!isPathMatch ? { pathOnly: true } : {}),
        ...(verbose && itemExtra.url ? { url: itemExtra.url } : {}),
      });
      continue;
    }

    let firstMatchForItem = true;
    let emittedMatchForItem = false;
    for (const m of item.matches) {
      if (!m.context) continue;
      const match: CodeSearchGroupedMatch = {
        path: item.path,
        value: truncateSnippetChars(m.context),
      };
      if (m.positions?.length > 0) {
        match.matchIndices = m.positions.map(([start, end]) => ({
          start,
          end,
          lineOffset:
            (m.context ?? '').substring(0, start).split('\n').length - 1,
        }));
      }
      if (verbose && firstMatchForItem && itemExtra.url) {
        match.url = itemExtra.url;
        firstMatchForItem = false;
      }
      group.matches.push(match);
      emittedMatchForItem = true;
    }

    if (!emittedMatchForItem) {
      group.matches.push({
        path: item.path,
        pathOnly: true,
        ...(verbose && itemExtra.url ? { url: itemExtra.url } : {}),
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
