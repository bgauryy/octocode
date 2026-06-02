/**
 * githubSearchCode finalizer.
 *
 * Tool-specific responsibilities only: read per-query code-search data, merge
 * owner/repo groups, and define code-search hint wording. Generic pagination,
 * error extraction, hint dedupe, and response formatting live in
 * utils/response/groupedFinalizer.ts so other grouped tools reuse them.
 *
 * Output is bound to the registered Zod schema via
 * `BulkFinalizer<TQuery, GitHubCodeSearchOutputLocal>` so any shape drift is
 * caught at compile time before reaching the MCP SDK validator.
 */
import type { BulkFinalizer } from '../../types/bulk.js';
import type { FlatQueryResult } from '../../types/toolResults.js';
import {
  applyBulkCharWindow,
  collectFlatErrors,
  dedupeHints,
  formatFinalizedResponse,
  paginateGroupsCharWindow,
  readNonNegativeNumber,
  readPositiveNumber,
  type CharPagination,
  type PerQueryPagination,
  type QueryWithPagination,
} from '../../utils/response/groupedFinalizer.js';
import type { GitHubCodeSearchOutputLocal } from '../../scheme/remoteSchemaOverlay.js';
import {
  isConcise,
  isCompact,
  compactTrimHints,
  makeAdvisoryPredicate,
} from '../../scheme/verbosity.js';
import type { WithVerbosity } from '../../scheme/localSchemaOverlay.js';
import { buildEvidenceMetadata } from '../evidence.js';

export const CONCISE_SEARCH_CODE_LIMIT = 3;

/** Advisory hints githubSearchCode emits; stripped under compact.
 * Substring-OR, case-insensitive. */
const isAdvisorySearchCodeHint = makeAdvisoryPredicate([
  'pivot term',
  'cross-repo search',
  'zero hits',
  'check repo structure',
]);
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
  pagination?: PerQueryPagination;
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

/** Escape a string for safe embedding in a RegExp. */
function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/**
 * Exactness tier for a match against the search keywords (higher = stronger):
 *   2 — the keyword names the file (basename, with or without extension)
 *   1 — the keyword appears as a whole token in the snippet (`createStore(`
 *       matches `createStore`, but `createStoreImpl` / `xCreateStore` do not);
 *       phrase/punctuation keywords fall back to a literal substring
 *   0 — no exact signal (GitHub's fuzzy hit stands)
 * A filename hit is a stronger signal than a body hit, so it outranks it.
 */
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
    if (base === lower || baseNoExt === lower) return 2; // filename — strongest
    const bodyHit = /^[A-Za-z0-9_]+$/.test(kw)
      ? new RegExp(`\\b${escapeRegExp(kw)}\\b`, 'i').test(value)
      : value.toLowerCase().includes(lower);
    if (bodyHit) score = Math.max(score, 1);
  }
  return score;
}

/**
 * Exact-match re-ranking layered on top of GitHub's ordering. Within each group
 * exact hits float to the top (stable otherwise); groups are then ordered by
 * (has-exact-hit, match-count, id). Pure tiebreaker — when no keyword matches
 * anything it degrades to {@link rankGroupsByRelevance}. Exported for tests.
 */
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
      if (a.score !== b.score) return b.score - a.score; // higher tier first
      return a.index - b.index; // otherwise stable
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

function getMatches(
  group: CodeSearchGroupedResult
): readonly CodeSearchGroupedMatch[] {
  return group.matches;
}

function setMatches(
  group: CodeSearchGroupedResult,
  matches: CodeSearchGroupedMatch[]
): CodeSearchGroupedResult {
  return { ...group, matches };
}

/** The single paginatable text field on a code-search match. */
const getMatchText = (match: CodeSearchGroupedMatch): string | undefined =>
  match.value;
const setMatchText = (
  match: CodeSearchGroupedMatch,
  value: string
): CodeSearchGroupedMatch => ({ ...match, value });

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
  perQueryPagination: readonly PerQueryPagination[],
  responsePagination: CharPagination | undefined,
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
  if (perQueryPagination.some(page => page.hasMore)) {
    reasons.push('One or more query-level char pages have more data.');
  }
  if (responsePagination?.hasMore) {
    reasons.push('Bulk response pagination has more data.');
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

function conciseMatchValue(value: string | undefined): string {
  if (!value) return '';
  const firstLine =
    value
      .split(/\r?\n/)
      .map(line => line.trim())
      .find(line => line.length > 0) ?? '';
  const maxLength = 160;
  return firstLine.length > maxLength
    ? `${firstLine.slice(0, maxLength - 1)}…`
    : firstLine;
}

function dedupeConciseMatchesByPath(
  matches: readonly CodeSearchGroupedMatch[]
): CodeSearchGroupedMatch[] {
  const seen = new Set<string>();
  const deduped: CodeSearchGroupedMatch[] = [];
  for (const match of matches) {
    if (seen.has(match.path)) continue;
    seen.add(match.path);
    deduped.push({
      ...match,
      value: conciseMatchValue(match.value),
    });
  }
  return deduped;
}

export function buildGithubSearchCodeFinalizer<
  TQuery extends QueryWithPagination,
>(): BulkFinalizer<TQuery, GitHubCodeSearchOutputLocal> {
  return ({ queries, results, config }) => {
    const perQueryGroups: PerQueryGroups[] = [];
    let upstreamPagination: CodeSearchPagination | undefined;
    let upstreamPaginationQueries = 0;

    const emptyQueries: Array<{ id: string; hints: string[] }> = [];

    results.forEach((res, index) => {
      if (res.status === 'error') return;

      const query = queries[index]!;
      const flat = readPerQueryFlat(res);
      // Capture zero-match queries before they get merged out of existence.
      // Without this, callers can't distinguish "merged into another
      // owner/repo group" from "this query produced nothing".
      const totalMatches = flat.results.reduce(
        (sum, group) => sum + group.matches.length,
        0
      );
      if (totalMatches === 0) {
        // Per-query empty hints flow through `data.hints` from
        // createSuccessResult(..., 'empty', { hintContext }) — pull them
        // forward so each emptyQueries[] entry tells the agent *why* this
        // specific query produced nothing.
        const rawHints = (res.data as { hints?: unknown }).hints;
        const perQueryHints = Array.isArray(rawHints)
          ? (rawHints as unknown[]).filter(
              (h): h is string => typeof h === 'string' && h.trim().length > 0
            )
          : [];
        emptyQueries.push({ id: res.id, hints: perQueryHints });
      }
      const requestedLength = readPositiveNumber(query.charLength);
      const requestedOffset = readNonNegativeNumber(query.charOffset);
      let groups = flat.results;
      let pagination: PerQueryPagination | undefined;

      if (
        groups.length > 0 &&
        (requestedLength !== undefined || requestedOffset !== undefined)
      ) {
        const sliced = paginateGroupsCharWindow({
          groups,
          getItems: getMatches,
          setItems: setMatches,
          getItemText: getMatchText,
          setItemText: setMatchText,
          charOffset: requestedOffset ?? 0,
          charLength: requestedLength ?? Number.MAX_SAFE_INTEGER,
        });
        groups = sliced.groups;
        pagination = { id: res.id, ...sliced.pagination };
      }

      perQueryGroups.push({ id: res.id, groups, pagination });

      if (flat.pagination) {
        upstreamPagination = flat.pagination;
        upstreamPaginationQueries += 1;
      }
    });

    // Exact-match re-ranking: boost whole-word / exact-filename hits over
    // GitHub's fuzzy ordering, using the keywords from every query in the bulk.
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
    let groups = applyExactMatchRanking(
      mergeGroups(perQueryGroups),
      allKeywords
    );
    const perQueryPagination = perQueryGroups
      .map(group => group.pagination)
      .filter((p): p is PerQueryPagination => p !== undefined);

    // Bulk char-pagination via the shared "explicit-or-overflow" policy.
    const bulk = applyBulkCharWindow(groups, config, {
      getItems: getMatches,
      setItems: setMatches,
      getItemText: getMatchText,
      setItemText: setMatchText,
    });
    groups = bulk.groups;
    const responsePagination = bulk.responsePagination;

    const paginationHints =
      upstreamPagination && upstreamPaginationQueries === 1
        ? buildPaginationHints(upstreamPagination, 'matches')
        : [];
    const continuationHints: string[] = [];
    for (const pagination of perQueryPagination) {
      if (!pagination.hasMore) continue;
      continuationHints.push(
        `Use charOffset=${pagination.charOffset + pagination.charLength} on query id=${pagination.id} to continue.`
      );
    }
    if (responsePagination?.hasMore) {
      continuationHints.push(
        `Use responseCharOffset=${responsePagination.charOffset + responsePagination.charLength} to continue this paginated bulk response.`
      );
    }

    const errors = collectFlatErrors(results);
    const hints = dedupeHints([
      ...(config.peerHints ? collectPeerHints(results) : []),
      ...paginationHints,
      ...continuationHints,
    ]);
    const responseData: GitHubCodeSearchOutputLocal = { results: groups };

    if (upstreamPagination && upstreamPaginationQueries === 1) {
      responseData.pagination = upstreamPagination;
    }
    if (perQueryPagination.length > 0)
      responseData.perQueryPagination = perQueryPagination;
    if (responsePagination)
      responseData.responsePagination = responsePagination;
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
        perQueryPagination,
        responsePagination,
        errors
      );
    }

    // ── Verbosity shaping ───────────────────────────────────────────────
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
        'perQueryPagination',
        'responsePagination',
        'hints',
        'emptyQueries',
        'errors',
      ],
      groups.length === 0 && errors.length > 0
    );
  };
}

/**
 * Per-tool verbosity shaping for githubSearchCode. Under concise (when every
 * query in the bulk opts in), caps groups to 3, keeps one line per path, and
 * emits a summary + drill-back hint. Under compact, advisory hints are trimmed
 * to 2. Basic / omitted / mixed bulks: passthrough.
 *
 * Mutates `responseData` in place; returns `true` when concise applied.
 */
export function applyGithubSearchCodeVerbosity(
  responseData: GitHubCodeSearchOutputLocal,
  queries: readonly QueryWithPagination[]
): boolean {
  const queriesWithVerbosity = queries as Array<
    WithVerbosity<QueryWithPagination>
  >;
  const allConcise =
    queriesWithVerbosity.length > 0 &&
    queriesWithVerbosity.every(q => isConcise(q.verbosity));
  const anyCompact = queriesWithVerbosity.some(q => isCompact(q.verbosity));
  const groups = (responseData.results ?? []) as CodeSearchGroupedResult[];

  if (allConcise) {
    const totalMatches = groups.reduce((n, g) => n + g.matches.length, 0);
    const distinctFiles = new Set(
      groups.flatMap(g => g.matches.map(m => m.path))
    ).size;
    const repoCount = groups.length;
    const topGroup = groups[0];
    const topPath = topGroup?.matches?.[0]?.path;
    const cappedGroups = groups.slice(0, CONCISE_SEARCH_CODE_LIMIT).map(g => ({
      ...g,
      matches: dedupeConciseMatchesByPath(g.matches),
    }));
    responseData.results = cappedGroups as typeof responseData.results;
    const topLoc = topPath
      ? ` (top: ${topGroup?.owner}/${topGroup?.repo}:${topPath})`
      : '';
    responseData.hints = [
      `${totalMatches} matches in ${distinctFiles} file(s) across ${repoCount} repo(s)${topLoc}`,
    ];
    // No verbosity-feature hint: concise's limit cap is its documented contract
    // and the match/file/repo totals above keep the full scope visible.
    return true;
  }

  if (anyCompact) {
    responseData.hints = compactTrimHints(
      responseData.hints,
      isAdvisorySearchCodeHint,
      2
    );
  }
  return false;
}
