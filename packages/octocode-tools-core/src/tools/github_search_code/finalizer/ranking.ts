import type { FlatQueryResult } from '../../../types/toolResults.js';
import { type QueryWithPagination } from '../../../utils/response/groupedFinalizer.js';
import type { RepoState } from '../execution.js';
import {
  type CodeSearchFlatResult,
  type CodeSearchGroupedMatch,
  type CodeSearchGroupedResult,
} from '../../providerMappers.js';

export function queryById(
  queries: readonly QueryWithPagination[]
): ReadonlyMap<string, QueryWithPagination> {
  const byId = new Map<string, QueryWithPagination>();
  for (const query of queries) {
    if (typeof query.id === 'string') byId.set(query.id, query);
  }
  return byId;
}

export function hasScopedGitHubQuery(
  emptyQueries: readonly { id: string }[],
  queries: readonly QueryWithPagination[]
): boolean {
  const queriesById = queryById(queries);
  return emptyQueries.some(empty => {
    const query = queriesById.get(empty.id) as
      (QueryWithPagination & { owner?: unknown; repo?: unknown }) | undefined;
    return typeof query?.owner === 'string' && typeof query?.repo === 'string';
  });
}

export function readPerQueryFlat(
  result: FlatQueryResult
): CodeSearchFlatResult & { repoState?: RepoState } {
  const data = result.data as
    (Partial<CodeSearchFlatResult> & { repoState?: RepoState }) | undefined;
  return {
    results: Array.isArray(data?.results) ? data.results : [],
    pagination: data?.pagination,
    ...(data?.nonExistentScope ? { nonExistentScope: true } : {}),
    ...(data?.incompleteResults ? { incompleteResults: true } : {}),
    ...(data?.repoState ? { repoState: data.repoState } : {}),
  };
}

function rankGroupsByRelevance(
  groups: readonly CodeSearchGroupedResult[]
): CodeSearchGroupedResult[] {
  return [...groups].sort((left, right) => {
    const matchDelta = right.matches.length - left.matches.length;
    if (matchDelta !== 0) return matchDelta;
    return left.id.localeCompare(right.id);
  });
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function matchExactnessScore(
  match: CodeSearchGroupedMatch,
  keywords: readonly string[]
): number {
  const base = (match.path.split('/').pop() ?? '').toLowerCase();
  const baseNoExt = base.replace(/\.[^.]+$/, '');
  const value = match.value ?? '';
  let score = 0;
  for (const raw of keywords) {
    const kw = raw.trim();
    if (!kw) continue;
    const lower = kw.toLowerCase();
    if (base === lower || baseNoExt === lower) return 2;
    const bodyHit = /^[A-Za-z0-9_]+$/.test(kw)
      ? new RegExp(`\\b${escapeRegExp(kw)}\\b`, 'i').test(value)
      : value.toLowerCase().includes(lower);
    if (bodyHit) score = Math.max(score, 1);
  }
  return score;
}

function setMatches(
  group: CodeSearchGroupedResult,
  matches: CodeSearchGroupedMatch[]
): CodeSearchGroupedResult {
  return { ...group, matches };
}

export function applyExactMatchRanking(
  groups: readonly CodeSearchGroupedResult[],
  keywords: readonly string[]
): CodeSearchGroupedResult[] {
  const terms = keywords.filter(k => typeof k === 'string' && k.trim());
  if (terms.length === 0) return rankGroupsByRelevance(groups);

  const withExactFirst = groups.map(group => {
    const scored = group.matches.map((match, index) => ({
      match,
      index,
      score: matchExactnessScore(match, terms),
    }));
    scored.sort((a, b) => {
      if (a.score !== b.score) return b.score - a.score;
      return a.index - b.index;
    });
    return {
      group: setMatches(
        group,
        scored.map(s => s.match)
      ),
      hasExact: scored.some(s => s.score > 0),
    };
  });

  return withExactFirst
    .sort((left, right) => {
      if (left.hasExact !== right.hasExact) return left.hasExact ? -1 : 1;
      const matchDelta = right.group.matches.length - left.group.matches.length;
      if (matchDelta !== 0) return matchDelta;
      return left.group.id.localeCompare(right.group.id);
    })
    .map(entry => entry.group);
}
