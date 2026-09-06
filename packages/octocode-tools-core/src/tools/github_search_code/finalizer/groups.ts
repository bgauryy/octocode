import { type QueryWithPagination } from '../../../utils/response/groupedFinalizer.js';
import type { ToolContinuation } from '../../../scheme/pagination.js';
import {
  type CodeSearchGroupedMatch,
  type CodeSearchGroupedResult,
  type CodeSearchPagination,
} from '../../providerMappers/codeSearch.js';
export type PerQueryGroups = {
  index: number;
  groups: CodeSearchGroupedResult[];
};

export type CodeSearchFileResult = {
  owner: string;
  repo: string;
  path: string;
  matches: Array<Omit<CodeSearchGroupedMatch, 'path'>>;
};

export type CodeSearchResultRecord = {
  index: number;
  data: {
    files: CodeSearchFileResult[];
    pagination?: CodeSearchPagination;
  };
};

export function mergeGroups(
  perQuery: readonly PerQueryGroups[]
): CodeSearchGroupedResult[] {
  const merged = new Map<string, CodeSearchGroupedResult>();
  for (const { index: queryIndex, groups } of perQuery) {
    for (const group of groups) {
      const mergeKey = `${queryIndex}\u0000${group.id}`;
      const existing = merged.get(mergeKey);
      if (!existing) {
        merged.set(mergeKey, {
          id: group.id,
          queryIndex,
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
      const key = `${group.queryIndex ?? -1}\u0000${group.owner}\u0000${group.repo}\u0000${match.path}`;
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
        // queryIndex intentionally omitted from each file: it always equals the
        // parent results[].index. It is still part of `key` above so files from
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
  paginationByQuery: ReadonlyMap<number, CodeSearchPagination>
): CodeSearchResultRecord[] {
  if (groups.length === 0) return [];

  // Single query: collapse to one record carrying that query's pagination.
  if (queries.length === 1) {
    const index = groups[0]?.queryIndex ?? 0;
    const pagination = paginationByQuery.get(index);
    return [
      {
        index,
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
  const byQuery = new Map<number, CodeSearchGroupedResult[]>();
  const order: number[] = [];
  for (const group of groups) {
    const queryIndex = group.queryIndex ?? 0;
    let bucket = byQuery.get(queryIndex);
    if (!bucket) {
      bucket = [];
      byQuery.set(queryIndex, bucket);
      order.push(queryIndex);
    }
    bucket.push(group);
  }

  return order.map(index => {
    const pagination = paginationByQuery.get(index);
    return {
      index,
      data: {
        files: flattenGroupsToFiles(byQuery.get(index)!),
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
  const next: Record<string, ToolContinuation> = {};
  for (const record of resultRecords) {
    const file = record.data.files[0];
    if (!file) continue;
    const query = queries[record.index] as
      | (QueryWithPagination & { keywords?: unknown; match?: unknown })
      | undefined;
    if (query?.match === 'path') continue;
    const ownKeywords = Array.isArray(query?.keywords)
      ? query.keywords.filter((k): k is string => typeof k === 'string')
      : [];
    const matchString = ownKeywords[0] ?? allKeywords[0];
    if (!matchString) continue;
    const key =
      resultRecords.length === 1 ? 'getLines' : `getLines:${record.index}`;
    next[key] = {
      tool: 'ghGetFileContent',
      query: {
        owner: file.owner,
        repo: file.repo,
        path: file.path,
        matchString,
      },
      why: 'GitHub code search returns no line numbers; fetch the top hit with matchString to get exact file:line anchors',
      confidence: 'low',
    };
  }
  return Object.keys(next).length > 0 ? next : undefined;
}
