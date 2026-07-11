import { type QueryWithPagination } from '../../../utils/response/groupedFinalizer.js';
import type { ToolContinuation } from '../../../scheme/pagination.js';
import {
  type CodeSearchGroupedMatch,
  type CodeSearchGroupedResult,
  type CodeSearchPagination,
} from '../../providerMappers.js';
import { queryById } from './ranking.js';

export type PerQueryGroups = {
  id: string;
  groups: CodeSearchGroupedResult[];
};

export type CodeSearchFileResult = {
  owner: string;
  repo: string;
  path: string;
  matches: Array<Omit<CodeSearchGroupedMatch, 'path'>>;
};

export type CodeSearchResultRecord = {
  id: string;
  data: {
    files: CodeSearchFileResult[];
    pagination?: CodeSearchPagination;
  };
};

export function mergeGroups(
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
        // queryId intentionally omitted from output: it always equals the
        // parent results[].id. It is still part of `key` above so files from
        // different queries never merge.
        matches: [matchWithoutPath],
      });
    }
  }
  return Array.from(byFile.values());
}

export function buildResultRecords(
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

// GitHub code search returns snippet fragments with NO absolute line numbers.
// For each result record, emit a ready-made ghGetFileContent call against the
// record's top file using the query's first keyword as matchString — one step
// to an exact file:line anchor instead of a clone-and-grep loop.
export function buildNextMap(
  resultRecords: readonly CodeSearchResultRecord[],
  queries: readonly QueryWithPagination[],
  allKeywords: readonly string[]
): Record<string, ToolContinuation> | undefined {
  const queriesById = queryById(queries);
  const next: Record<string, ToolContinuation> = {};
  for (const record of resultRecords) {
    const file = record.data.files[0];
    if (!file) continue;
    const query = queriesById.get(record.id) as
      (QueryWithPagination & { keywords?: unknown }) | undefined;
    const ownKeywords = Array.isArray(query?.keywords)
      ? query.keywords.filter((k): k is string => typeof k === 'string')
      : [];
    const matchString = ownKeywords[0] ?? allKeywords[0];
    if (!matchString) continue;
    const key =
      resultRecords.length === 1 ? 'getLines' : `getLines:${record.id}`;
    next[key] = {
      tool: 'ghGetFileContent',
      query: {
        owner: file.owner,
        repo: file.repo,
        path: file.path,
        matchString,
      },
      why: 'GitHub code search returns no line numbers; fetch the top hit with matchString to get exact file:line anchors',
      confidence: 'heuristic',
    };
  }
  return Object.keys(next).length > 0 ? next : undefined;
}
