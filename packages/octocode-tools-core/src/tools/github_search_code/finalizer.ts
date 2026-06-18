import type { BulkFinalizer } from '../../types/bulk.js';
import type { FlatQueryResult } from '../../types/toolResults.js';
import {
  collectFlatErrors,
  dedupeHints,
  formatFinalizedResponse,
  type QueryWithPagination,
} from '../../utils/response/groupedFinalizer.js';
import type { GitHubCodeSearchOutputLocal } from './scheme.js';
import {
  buildPaginationHints,
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
  id: string;
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
        id: `${group.owner}/${group.repo}:${match.path}`,
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
  pagination: CodeSearchPagination | undefined
): CodeSearchResultRecord[] {
  if (groups.length === 0) return [];
  const id =
    queries.length === 1 && typeof queries[0]?.id === 'string'
      ? queries[0].id
      : 'ghSearchCode';
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

function collectPeerHints(results: readonly FlatQueryResult[]): string[] {
  return dedupeHints(
    results.flatMap(result => {
      const raw = result.data.hints;
      return Array.isArray(raw)
        ? raw.filter((hint): hint is string => typeof hint === 'string')
        : [];
    })
  );
}

function hasPathOnlyFileMatches(
  groups: readonly CodeSearchGroupedResult[]
): boolean {
  return groups.some(group => group.matches.some(match => match.pathOnly));
}

export function buildGhSearchCodeFinalizer<
  TQuery extends QueryWithPagination,
>(): BulkFinalizer<TQuery, GitHubCodeSearchOutputLocal> {
  return ({ queries, results, config }) => {
    const perQueryGroups: PerQueryGroups[] = [];
    let upstreamPagination: CodeSearchPagination | undefined;
    let upstreamPaginationQueries = 0;

    const emptyQueries: Array<{
      id: string;
      hints: string[];
      nonExistentScope?: true;
    }> = [];

    results.forEach((res, _index) => {
      if (res.status === 'error') return;

      const flat = readPerQueryFlat(res);
      const totalMatches = flat.results.reduce(
        (sum, group) => sum + group.matches.length,
        0
      );
      if (totalMatches === 0) {
        const rawHints = (res.data as { hints?: unknown }).hints;
        const perQueryHints = Array.isArray(rawHints)
          ? (rawHints as unknown[]).filter(
              (h): h is string => typeof h === 'string' && h.trim().length > 0
            )
          : [];
        emptyQueries.push({
          id: res.id,
          hints: perQueryHints,
          ...(flat.nonExistentScope ? { nonExistentScope: true as const } : {}),
        });
      }
      const groups = flat.results;
      perQueryGroups.push({ id: res.id, groups });

      if (flat.pagination) {
        upstreamPagination = flat.pagination;
        upstreamPaginationQueries += 1;
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

    const paginationHints =
      upstreamPagination && upstreamPaginationQueries === 1
        ? buildPaginationHints(upstreamPagination, 'matches')
        : [];

    const errors = collectFlatErrors(results);
    const pathOnlyHints = hasPathOnlyFileMatches(groups)
      ? [
          'Some match="file" results are path-only because GitHub did not return text matches; use ghGetFileContent with matchString or startLine/endLine to inspect content.',
        ]
      : [];
    const hints = dedupeHints([
      ...(config.peerHints ? collectPeerHints(results) : []),
      ...paginationHints,
      ...pathOnlyHints,
    ]);
    const resultPagination =
      upstreamPagination && upstreamPaginationQueries === 1
        ? upstreamPagination
        : undefined;
    const conciseMode = queries.some(
      q => (q as { concise?: boolean }).concise === true
    );
    const resultRecords = buildResultRecords(queries, groups, resultPagination);
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

    if (hints.length > 0) responseData.hints = hints;
    if (emptyQueries.length > 0) {
      const topLevelHints = new Set(hints);
      responseData.emptyQueries = emptyQueries.map(
        ({ id, hints: queryHints, nonExistentScope }) => {
          const uniqueHints = queryHints.filter(h => !topLevelHints.has(h));
          return {
            id,
            ...(uniqueHints.length > 0 ? { hints: uniqueHints } : {}),
            ...(nonExistentScope ? { nonExistentScope } : {}),
          };
        }
      );
    }
    if (errors.length > 0) responseData.errors = errors;

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
        'hints',
        'emptyQueries',
        'errors',
      ],
      groups.length === 0 && errors.length > 0
    );
  };
}
