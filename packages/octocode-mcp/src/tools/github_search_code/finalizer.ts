import type { BulkFinalizer } from '../../types/bulk.js';
import type { FlatQueryResult } from '../../types/toolResults.js';
import {
  collectFlatErrors,
  dedupeHints,
  formatFinalizedResponse,
  type QueryWithPagination,
} from '../../utils/response/groupedFinalizer.js';
import type { GitHubCodeSearchOutputLocal } from '../../scheme/remoteSchemaOverlay.js';
import { isVerbose } from '../../scheme/verbosity.js';
import type { WithVerbosity } from '../../scheme/localSchemaOverlay.js';
import { buildEvidenceMetadata } from '../evidence.js';
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

function readPerQueryFlat(result: FlatQueryResult): CodeSearchFlatResult {
  const data = result.data as Partial<CodeSearchFlatResult> | undefined;
  return {
    results: Array.isArray(data?.results) ? data.results : [],
    pagination: data?.pagination,
  };
}

function mergeGroups(
  perQuery: readonly PerQueryGroups[]
): CodeSearchGroupedResult[] {
  const merged = new Map<string, CodeSearchGroupedResult>();
  for (const { groups } of perQuery) {
    for (const group of groups) {
      const existing = merged.get(group.id);
      if (!existing) {
        merged.set(group.id, {
          id: group.id,
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

function buildCodeEvidence(
  groups: readonly CodeSearchGroupedResult[],
  upstreamPagination: CodeSearchPagination | undefined,
  errors: readonly { id: string; error: string }[]
): NonNullable<GitHubCodeSearchOutputLocal['evidence']> {
  const totalMatches = groups.reduce(
    (sum, group) => sum + group.matches.length,
    0
  );
  const reasons: string[] = [];

  if (upstreamPagination?.hasMore) {
    reasons.push('GitHub search pagination has more matches.');
  }
  if (errors.length > 0) {
    reasons.push(`${errors.length} query result(s) failed.`);
  }

  return buildEvidenceMetadata({
    kind: 'code',
    answerReady: totalMatches > 0,
    incompleteReasons: reasons,
    emptyReason: 'No code matches were returned for the supplied filters.',
  });
}

export function buildGithubSearchCodeFinalizer<
  TQuery extends QueryWithPagination,
>(): BulkFinalizer<TQuery, GitHubCodeSearchOutputLocal> {
  return ({ queries, results, config }) => {
    const perQueryGroups: PerQueryGroups[] = [];
    let upstreamPagination: CodeSearchPagination | undefined;
    let upstreamPaginationQueries = 0;

    const emptyQueries: Array<{ id: string; hints: string[] }> = [];

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
        emptyQueries.push({ id: res.id, hints: perQueryHints });
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
          const kws = (q as { keywordsToSearch?: unknown }).keywordsToSearch;
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
    const hints = dedupeHints([
      ...(config.peerHints ? collectPeerHints(results) : []),
      ...paginationHints,
    ]);
    const responseData: GitHubCodeSearchOutputLocal = { results: groups };

    if (upstreamPagination && upstreamPaginationQueries === 1) {
      responseData.pagination = upstreamPagination;
    }
    if (hints.length > 0) responseData.hints = hints;
    if (emptyQueries.length > 0) {
      responseData.emptyQueries = emptyQueries.map(({ id, hints }) =>
        hints.length > 0 ? { id, hints } : { id }
      );
    }
    if (errors.length > 0) responseData.errors = errors;
    if (config.peerEvidence) {
      responseData.evidence = buildCodeEvidence(
        groups,
        upstreamPagination,
        errors
      );
    }

    applyGithubSearchCodeVerbosity(responseData, queries);

    return formatFinalizedResponse<GitHubCodeSearchOutputLocal>(
      responseData,
      [
        'results',
        'id',
        'owner',
        'repo',
        'matches',
        'pagination',
        'hints',
        'emptyQueries',
        'errors',
      ],
      groups.length === 0 && errors.length > 0
    );
  };
}

export function applyGithubSearchCodeVerbosity(
  responseData: GitHubCodeSearchOutputLocal,
  queries: readonly QueryWithPagination[]
): void {
  const queriesTyped = queries as Array<WithVerbosity<QueryWithPagination>>;
  const anyVerbose = queriesTyped.some(q => isVerbose(q));
  if (anyVerbose) return;

  responseData.results = (responseData.results ?? []).map(g => ({
    ...g,
    matches: g.matches.map(m => {
      const { matchIndices: _mi, ...rest } = m as typeof m & {
        matchIndices?: unknown;
      };
      void _mi;
      return rest;
    }),
  })) as typeof responseData.results;
}
