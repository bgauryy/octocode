import type { BulkFinalizer } from '../../types/bulk.js';
import type { FlatQueryResult } from '../../types/toolResults.js';
import { formatFinalizedResponse } from '../../utils/response/groupedFinalizer.js';
import type {
  GitHubFetchContentData,
  GitHubFetchContentErrorData,
  GitHubFetchContentOutputLocal,
} from './scheme.js';
import { readDirectoryEntry, readFileEntry } from './finalizer/entryParsers.js';
import type { PartialFileContentQuery } from './finalizer/types.js';

function attachQueryContext(
  result: FlatQueryResult,
  query: PartialFileContentQuery | undefined
): GitHubFetchContentData | GitHubFetchContentErrorData {
  const owner = typeof query?.owner === 'string' ? query.owner : undefined;
  const repo = typeof query?.repo === 'string' ? query.repo : undefined;

  if (result.status === 'error') {
    const rawError = result.data.error;
    const error =
      typeof rawError === 'string' ? rawError : 'File content query failed';
    const isNotFound = /\b404\b|not found/i.test(error);
    return {
      ...(owner ? { owner } : {}),
      ...(repo ? { repo } : {}),
      ...(query?.path ? { path: String(query.path) } : {}),
      error:
        isNotFound && owner && repo
          ? `${error} — verify the path (exact case, no leading slash) and branch; list the tree with ghViewRepoStructure(owner:"${owner}", repo:"${repo}")`
          : error,
    };
  }

  const data: GitHubFetchContentData = {
    owner: owner ?? '',
    repo: repo ?? '',
  };
  if (query?.type === 'directory') {
    data.directories = [readDirectoryEntry(result.data, query)];
  } else {
    data.files = [readFileEntry(result.data, query ?? {})];
  }
  return data;
}

export function buildGithubFetchContentFinalizer<
  TQuery extends PartialFileContentQuery,
>(): BulkFinalizer<TQuery, GitHubFetchContentOutputLocal> {
  return ({ queries, results }) => {
    const rows = results.map(result => ({
      ...result,
      data: attachQueryContext(result, queries[result.index]),
    }));
    const responseData: GitHubFetchContentOutputLocal = { results: rows };

    return formatFinalizedResponse<GitHubFetchContentOutputLocal>(
      responseData,
      [
        'results',
        'index',
        'status',
        'meta',
        'data',
        'owner',
        'repo',
        'files',
        'directories',
        'path',
        'content',
        'fileType',
        'totalLines',
        'startLine',
        'endLine',
        'isPartial',
        'pagination',
        'error',
      ],
      rows.length > 0 && rows.every(row => row.status === 'error')
    );
  };
}
