import type { BulkFinalizer } from '../../types/bulk.js';
import type { FlatQueryResult } from '../../types/toolResults.js';
import {
  collectFlatErrors,
  formatFinalizedResponse,
} from '../../utils/response/groupedFinalizer.js';
import { resolveUniqueQueryIds } from '../../utils/response/bulk.js';
import type { GitHubFetchContentOutputLocal } from './scheme.js';
import { readDirectoryEntry, readFileEntry } from './finalizer/entryParsers.js';
import type {
  FileContentResponse,
  PartialFileContentQuery,
  RepoGroup,
  RepoGroupData,
} from './finalizer/types.js';

function groupId(owner: string, repo: string): string {
  return `${owner}/${repo}`;
}

function ensureGroup(
  groups: Map<string, RepoGroup>,
  owner: string,
  repo: string
): RepoGroup {
  const id = groupId(owner, repo);
  const existing = groups.get(id);
  if (existing) return existing;
  const created: RepoGroup = { id, owner, repo };
  groups.set(id, created);
  return created;
}

function buildGroups(
  results: readonly FlatQueryResult[],
  queryById: ReadonlyMap<string, PartialFileContentQuery>
): Array<{ id: string; data: RepoGroupData }> {
  const groups = new Map<string, RepoGroup>();

  results.forEach(result => {
    if (result.status === 'error') return;
    const query = queryById.get(result.id);
    if (!query) return;
    const owner = String(query.owner ?? '');
    const repo = String(query.repo ?? '');
    if (!owner || !repo) return;

    const group = ensureGroup(groups, owner, repo);
    const data = result.data;

    if (query.type === 'directory') {
      const directories = group.directories ?? [];
      directories.push(readDirectoryEntry(data, query));
      group.directories = directories;
      return;
    }

    const files = group.files ?? [];
    files.push(readFileEntry(data, query));
    group.files = files;
  });

  return Array.from(groups.values()).map(group => {
    const data: RepoGroupData = {
      owner: group.owner,
      repo: group.repo,
      ...(group.files ? { files: group.files } : {}),
      ...(group.directories ? { directories: group.directories } : {}),
    };
    // Emit only { id, data } — the canonical row shape. owner/repo/files/
    // directories live ONLY under data (previously also mirrored flat at the
    // top level, byte-identical, which doubled file-content payloads).
    return { id: group.id, data };
  });
}

function collectFileErrors(
  results: readonly FlatQueryResult[],
  queryById: ReadonlyMap<string, PartialFileContentQuery>
): FileContentResponse['errors'] {
  const base = collectFlatErrors(results);
  return base.map(error => {
    const query = queryById.get(error.id);
    // A 404 conflates bad path, bad branch, and missing repo — attach the
    // recovery route in-band instead of only in the static tool description.
    const isNotFound = /\b404\b|not found/i.test(error.error);
    const message =
      isNotFound && query?.owner && query?.repo
        ? `${error.error} — verify the path (exact case, no leading slash) and the branch (a bad ref also 404s); list the tree with ghViewRepoStructure(owner:"${query.owner}", repo:"${query.repo}")`
        : error.error;
    return {
      id: error.id,
      owner: query?.owner,
      repo: query?.repo,
      path: query?.path ? String(query.path) : undefined,
      error: message,
    };
  });
}

export function buildGithubFetchContentFinalizer<
  TQuery extends PartialFileContentQuery,
>(): BulkFinalizer<TQuery, GitHubFetchContentOutputLocal> {
  return ({ queries, results }) => {
    // Align each flat result row to its originating query by id (the same
    // batch-unique ids bulk.ts derives via resolveUniqueQueryIds), not by
    // array position — a dropped or reordered query would otherwise attach
    // the wrong owner/repo/path, and duplicate explicit ids would collide.
    const queryById = new Map<string, PartialFileContentQuery>();
    const uniqueIds = resolveUniqueQueryIds(queries);
    queries.forEach((query, index) => {
      queryById.set(uniqueIds[index]!, query);
    });

    const groups = buildGroups(results, queryById);

    const errors = collectFileErrors(results, queryById);
    const responseData: FileContentResponse = { results: groups };

    if (errors && errors.length > 0) responseData.errors = errors;

    return formatFinalizedResponse<GitHubFetchContentOutputLocal>(
      responseData,
      [
        'results',
        'id',
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
        'errors',
      ],
      groups.length === 0 && Boolean(errors && errors.length > 0)
    );
  };
}
