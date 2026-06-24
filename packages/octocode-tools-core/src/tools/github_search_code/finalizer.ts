import type { BulkFinalizer } from '../../types/bulk.js';
import type { FlatQueryResult } from '../../types/toolResults.js';
import {
  collectFlatErrors,
  formatFinalizedResponse,
  type QueryWithPagination,
} from '../../utils/response/groupedFinalizer.js';
import type { GitHubCodeSearchOutputLocal } from './scheme.js';
import {
  type CodeSearchFlatResult,
  type CodeSearchGroupedMatch,
  type CodeSearchGroupedResult,
  type CodeSearchPagination,
} from '../providerMappers.js';

type PerQueryGroups = {
  id: string;
  groups: CodeSearchGroupedResult[];
};

type CodeSearchFileResult = {
  owner: string;
  repo: string;
  path: string;
  queryId?: string;
  matches: Array<Omit<CodeSearchGroupedMatch, 'path'>>;
};

type CodeSearchResultRecord = {
  id: string;
  data: {
    files: CodeSearchFileResult[];
    pagination?: CodeSearchPagination;
  };
};

function readPerQueryFlat(result: FlatQueryResult): CodeSearchFlatResult {
  const data = result.data as Partial<CodeSearchFlatResult> | undefined;
  return {
    results: Array.isArray(data?.results) ? data.results : [],
    pagination: data?.pagination,
    ...(data?.nonExistentScope ? { nonExistentScope: true } : {}),
    ...(data?.incompleteResults ? { incompleteResults: true } : {}),
  };
}

function mergeGroups(
  perQuery: readonly PerQueryGroups[]
): CodeSearchGroupedResult[] {
  const merged = new Map<string, CodeSearchGroupedResult>();
  for (const { id: queryId, groups } of perQuery) {
    for (const group of groups) {
      const mergeKey = `${queryId}\u0000${group.id}`;
      const existing = merged.get(mergeKey);
      if (!existing) {
        merged.set(mergeKey, {
          id: group.id,
          queryId,
          owner: group.owner,
          repo: group.repo,
          matches: [...group.matches],
        });
        continue;
      }
      existing.matches.push(...group.matches);
    }
  }
  return Array.from(merged.values());
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

function flattenGroupsToFiles(
  groups: readonly CodeSearchGroupedResult[]
): CodeSearchFileResult[] {
  const byFile = new Map<string, CodeSearchFileResult>();
  for (const group of groups) {
    for (const match of group.matches) {
      const key = `${group.queryId ?? ''}\u0000${group.owner}\u0000${group.repo}\u0000${match.path}`;
      const existing = byFile.get(key);
      const { path: _path, ...matchWithoutPath } = match;
      if (existing) {
        existing.matches.push(matchWithoutPath);
        continue;
      }
      byFile.set(key, {
        owner: group.owner,
        repo: group.repo,
        path: match.path,
        ...(group.queryId ? { queryId: group.queryId } : {}),
        matches: [matchWithoutPath],
      });
    }
  }
  return Array.from(byFile.values());
}

function buildResultRecords(
  queries: readonly QueryWithPagination[],
  groups: readonly CodeSearchGroupedResult[],
  paginationByQuery: ReadonlyMap<string, CodeSearchPagination>
): CodeSearchResultRecord[] {
  if (groups.length === 0) return [];

  // Single query: collapse to one record keyed by the query id (or the tool
  // name), carrying that query's pagination — identical to the prior shape.
  if (queries.length === 1) {
    const id =
      typeof queries[0]?.id === 'string' ? queries[0].id : 'ghSearchCode';
    const onlyId =
      typeof queries[0]?.id === 'string' ? queries[0].id : undefined;
    const pagination = onlyId ? paginationByQuery.get(onlyId) : undefined;
    return [
      {
        id,
        data: {
          files: flattenGroupsToFiles(groups),
          ...(pagination ? { pagination } : {}),
        },
      },
    ];
  }

  // Multi-query bulk: emit one record PER query that produced results, each
  // carrying its OWN pagination so an agent can page deeper on every query
  // independently (previously the merged block dropped all but one).
  const byQuery = new Map<string, CodeSearchGroupedResult[]>();
  const order: string[] = [];
  for (const group of groups) {
    const queryId = group.queryId ?? 'ghSearchCode';
    let bucket = byQuery.get(queryId);
    if (!bucket) {
      bucket = [];
      byQuery.set(queryId, bucket);
      order.push(queryId);
    }
    bucket.push(group);
  }

  return order.map(queryId => {
    const pagination = paginationByQuery.get(queryId);
    return {
      id: queryId,
      data: {
        files: flattenGroupsToFiles(byQuery.get(queryId)!),
        ...(pagination ? { pagination } : {}),
      },
    };
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

function setMatches(
  group: CodeSearchGroupedResult,
  matches: CodeSearchGroupedMatch[]
): CodeSearchGroupedResult {
  return { ...group, matches };
}

export function buildGhSearchCodeFinalizer<
  TQuery extends QueryWithPagination,
>(): BulkFinalizer<TQuery, GitHubCodeSearchOutputLocal> {
  return ({ queries, results }) => {
    const perQueryGroups: PerQueryGroups[] = [];
    const paginationByQuery = new Map<string, CodeSearchPagination>();

    const emptyQueries: Array<{
      id: string;
      nonExistentScope?: true;
      incompleteResults?: true;
    }> = [];
    let anyIncompleteResults = false;

    results.forEach((res, _index) => {
      if (res.status === 'error') return;

      const flat = readPerQueryFlat(res);
      if (flat.incompleteResults) anyIncompleteResults = true;
      const totalMatches = flat.results.reduce(
        (sum, group) => sum + group.matches.length,
        0
      );
      if (totalMatches === 0) {
        emptyQueries.push({
          id: res.id,
          ...(flat.nonExistentScope ? { nonExistentScope: true as const } : {}),
          ...(flat.incompleteResults
            ? { incompleteResults: true as const }
            : {}),
        });
      }
      const groups = flat.results;
      perQueryGroups.push({ id: res.id, groups });

      if (flat.pagination) {
        paginationByQuery.set(res.id, flat.pagination);
      }
    });

    const allKeywords = Array.from(
      new Set(
        queries.flatMap(q => {
          const kws = (q as { keywords?: unknown }).keywords;
          return Array.isArray(kws)
            ? kws.filter((k): k is string => typeof k === 'string')
            : [];
        })
      )
    );
    const groups = applyExactMatchRanking(
      mergeGroups(perQueryGroups),
      allKeywords
    );

    const errors = collectFlatErrors(results);
    const conciseMode = queries.some(
      q => (q as { concise?: boolean }).concise === true
    );
    const resultRecords = buildResultRecords(
      queries,
      groups,
      paginationByQuery
    );
    if (conciseMode) {
      for (const rec of resultRecords) {
        rec.data.files = rec.data.files.map(
          f => `${f.owner}/${f.repo}:${f.path}`
        ) as unknown as typeof rec.data.files;
      }
    }
    const responseData: GitHubCodeSearchOutputLocal = {
      results: resultRecords,
    };

    if (emptyQueries.length > 0) {
      responseData.emptyQueries = emptyQueries.map(
        ({ id, nonExistentScope, incompleteResults }) => ({
          id,
          ...(nonExistentScope ? { nonExistentScope } : {}),
          ...(incompleteResults ? { incompleteResults } : {}),
        })
      );
    }
    if (errors.length > 0) responseData.errors = errors;

    // GitHub's index did not fully complete for at least one query — empty or
    // partial results may be a false negative, NOT a true absence. Surface it
    // as a visible warning so the agent distinguishes "no match" from "search
    // degraded" and can retry, narrow scope, or search the repo locally.
    if (anyIncompleteResults) {
      responseData.warnings = [
        ...(Array.isArray(responseData.warnings) ? responseData.warnings : []),
        'GitHub code search returned incomplete_results: the search index did not fully complete. Empty or partial results may be a false negative — retry, narrow scope (owner/repo/path), or materialize the repo and search locally before concluding absence.',
      ];
    }

    return formatFinalizedResponse<GitHubCodeSearchOutputLocal>(
      responseData,
      [
        'results',
        'id',
        'data',
        'files',
        'path',
        'owner',
        'repo',
        'queryId',
        'matches',
        'value',
        'pathOnly',
        'matchIndices',
        'pagination',
        'emptyQueries',
        'nonExistentScope',
        'incompleteResults',
        'warnings',
        'errors',
      ],
      groups.length === 0 && errors.length > 0
    );
  };
}
