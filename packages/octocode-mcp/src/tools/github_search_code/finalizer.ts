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
import { tsvFormat } from '../../utils/response/tsvFormat.js';
import { getTsvProjection } from '../../utils/response/tsvColumns.js';
import { finalizeTsv } from '../../utils/response/tsvFinalize.js';
import { STATIC_TOOL_NAMES } from '../toolNames.js';

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
  outputPagination: readonly PerQueryPagination[],
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
  if (outputPagination.some(page => page.hasMore)) {
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

    let groups = rankGroupsByRelevance(mergeGroups(perQueryGroups));
    const outputPagination = perQueryGroups
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
    for (const pagination of outputPagination) {
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
    if (outputPagination.length > 0)
      responseData.outputPagination = outputPagination;
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
        outputPagination,
        responsePagination,
        errors
      );
    }

    // ── Verbosity shaping ───────────────────────────────────────────────
    const allConcise = applyGithubSearchCodeVerbosity(responseData, queries);

    // TSV branch — render flattened rows from the merged groups and attach
    // the columns/rows pair next to `results`. Callers can read either.
    // Skip under all-concise: no rows worth emitting.
    if (config.format === 'tsv' && !allConcise) {
      const projection = getTsvProjection(STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE);
      if (projection) {
        const lean = finalizeTsv(
          projection.columns,
          Array.from(projection.toRows({ results: groups }))
        );
        responseData.format = 'tsv';
        responseData.columns = lean.columns;
        responseData.rows = tsvFormat(lean.columns, lean.rows);
        if (lean.base) responseData.base = lean.base;
        if (lean.shared) responseData.shared = lean.shared;
      }
    }

    return formatFinalizedResponse<GitHubCodeSearchOutputLocal>(
      responseData,
      [
        'format',
        'columns',
        'rows',
        'results',
        'id',
        'owner',
        'repo',
        'matches',
        'pagination',
        'outputPagination',
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
 * Mutates `responseData` in place; returns `true` when concise applied so the
 * caller can skip TSV emission.
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
