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
  collectFlatErrors,
  dedupeHints,
  formatFinalizedResponse,
  paginateGroupsWithNestedItemEscape,
  paginateNestedItems,
  readNonNegativeNumber,
  readPositiveNumber,
  type CharPagination,
  type PerQueryPagination,
  type QueryWithPagination,
} from '../../utils/response/groupedFinalizer.js';
import type {
  GitHubCodeSearchOutputLocal,
  GitHubCodeSearchWarning,
} from '../../scheme/remoteSchemaOverlay.js';
import {
  buildPaginationHints,
  type CodeSearchFlatResult,
  type CodeSearchGroupedMatch,
  type CodeSearchGroupedResult,
  type CodeSearchPagination,
} from '../providerMappers.js';
import { tsvFormat } from '../../utils/response/tsvFormat.js';
import { getTsvProjection } from '../../utils/response/tsvColumns.js';
import { STATIC_TOOL_NAMES } from '../toolNames.js';

type PerQueryGroups = {
  id: string;
  groups: CodeSearchGroupedResult[];
  pagination?: PerQueryPagination;
};

type TruncationWarning = Extract<
  GitHubCodeSearchWarning,
  { kind: 'match-value-truncated' }
>;

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

function truncateOversizedMatch(
  match: CodeSearchGroupedMatch,
  charLength: number,
  group: CodeSearchGroupedResult,
  warnings: TruncationWarning[]
): CodeSearchGroupedMatch {
  if (!match.value) return match;
  const marker = '… [truncated]';
  const valueBudget = Math.max(
    0,
    charLength - match.path.length - marker.length - 64
  );
  if (match.value.length <= valueBudget) return match;
  warnings.push({
    kind: 'match-value-truncated',
    groupId: group.id,
    path: match.path,
    fullValueLength: match.value.length,
    truncatedAt: valueBudget,
    recovery:
      'Retry with larger responseCharLength or narrow the search to this file/path.',
  });
  return {
    ...match,
    value: `${match.value.slice(0, valueBudget)}${marker}`,
  };
}

export function buildGithubSearchCodeFinalizer<
  TQuery extends QueryWithPagination,
>(): BulkFinalizer<TQuery, GitHubCodeSearchOutputLocal> {
  return ({ queries, results, config }) => {
    const perQueryGroups: PerQueryGroups[] = [];
    const warnings: TruncationWarning[] = [];
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
        const sliced = paginateNestedItems({
          groups,
          getItems: getMatches,
          setItems: setMatches,
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

    let groups = mergeGroups(perQueryGroups);
    const outputPagination = perQueryGroups
      .map(group => group.pagination)
      .filter((p): p is PerQueryPagination => p !== undefined);

    let responsePagination: CharPagination | undefined;
    if (
      groups.length > 0 &&
      (config.responseCharLength !== undefined ||
        config.responseCharOffset !== undefined)
    ) {
      const sliced = paginateGroupsWithNestedItemEscape({
        groups,
        getItems: getMatches,
        setItems: setMatches,
        charOffset: config.responseCharOffset ?? 0,
        charLength: config.responseCharLength ?? Number.MAX_SAFE_INTEGER,
        truncateOversizedItem: (match, charLength, group) =>
          truncateOversizedMatch(match, charLength, group, warnings),
      });
      groups = sliced.groups;
      responsePagination = sliced.pagination;
    }

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

    const hints = dedupeHints([...paginationHints, ...continuationHints]);
    const errors = collectFlatErrors(results);
    const responseData: GitHubCodeSearchOutputLocal = { results: groups };

    if (upstreamPagination && upstreamPaginationQueries === 1) {
      responseData.pagination = upstreamPagination;
    }
    if (outputPagination.length > 0)
      responseData.outputPagination = outputPagination;
    if (responsePagination)
      responseData.responsePagination = responsePagination;
    if (hints.length > 0) responseData.hints = hints;
    if (warnings.length > 0) responseData.warnings = warnings;
    if (emptyQueries.length > 0) {
      responseData.emptyQueries = emptyQueries.map(({ id, hints }) =>
        hints.length > 0 ? { id, hints } : { id }
      );
    }
    if (errors.length > 0) responseData.errors = errors;

    // TSV branch — render flattened rows from the merged groups and attach
    // the columns/rows pair next to `results`. Callers can read either.
    if (config.format === 'tsv') {
      const projection = getTsvProjection(STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE);
      if (projection) {
        responseData.format = 'tsv';
        responseData.columns = [...projection.columns];
        responseData.rows = tsvFormat(
          projection.columns,
          projection.toRows({ results: groups })
        );
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
        'warnings',
        'emptyQueries',
        'errors',
      ],
      groups.length === 0 && errors.length > 0
    );
  };
}
