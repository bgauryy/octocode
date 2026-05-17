import type {
  GitLabCodeSearchItem,
  GitLabFileContent,
  GitLabMergeRequest,
  GitLabMRNote,
  GitLabProject,
  GitLabTreeItem,
} from './types.js';

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function readString(record: Record<string, unknown>, key: string): string {
  const value = record[key];
  return typeof value === 'string' ? value : '';
}

function readNumber(record: Record<string, unknown>, key: string): number {
  const value = record[key];
  return typeof value === 'number' ? value : 0;
}

export function parseGitLabArray<T>(
  value: unknown,
  itemGuard: (item: unknown) => item is T
): T[] | null {
  return Array.isArray(value) ? value.filter(itemGuard) : null;
}

export function isGitLabCodeSearchItem(
  value: unknown
): value is GitLabCodeSearchItem {
  return (
    isRecord(value) &&
    typeof value.path === 'string' &&
    typeof value.data === 'string' &&
    typeof value.filename === 'string' &&
    typeof value.ref === 'string' &&
    typeof value.startline === 'number' &&
    typeof value.project_id === 'number'
  );
}

export function isGitLabProject(value: unknown): value is GitLabProject {
  return (
    isRecord(value) &&
    typeof value.id === 'number' &&
    typeof value.name === 'string' &&
    typeof value.path === 'string' &&
    typeof value.path_with_namespace === 'string'
  );
}

export function parseGitLabFileContent(
  value: unknown,
  fallbackRef: string
): GitLabFileContent | null {
  if (!isRecord(value)) return null;
  let content = readString(value, 'content');
  if (value.encoding === 'base64') {
    content = Buffer.from(content, 'base64').toString('utf-8');
  }
  return {
    file_name: readString(value, 'file_name'),
    file_path: readString(value, 'file_path'),
    size: readNumber(value, 'size'),
    encoding: 'utf-8',
    content,
    content_sha256: readString(value, 'content_sha256'),
    ref: readString(value, 'ref') || fallbackRef,
    blob_id: readString(value, 'blob_id'),
    commit_id: readString(value, 'commit_id'),
    last_commit_id: readString(value, 'last_commit_id'),
    execute_filemode: Boolean(value.execute_filemode),
  };
}

export function parseGitLabDefaultBranch(value: unknown): string | null {
  if (!isRecord(value)) return null;
  return typeof value.default_branch === 'string' ? value.default_branch : null;
}

export function isGitLabMergeRequest(
  value: unknown
): value is GitLabMergeRequest {
  return (
    isRecord(value) &&
    typeof value.iid === 'number' &&
    typeof value.title === 'string' &&
    typeof value.state === 'string'
  );
}

export function isGitLabMRNote(value: unknown): value is GitLabMRNote {
  return (
    isRecord(value) &&
    typeof value.id === 'number' &&
    typeof value.body === 'string' &&
    typeof value.system === 'boolean' &&
    isRecord(value.author)
  );
}

export function isGitLabTreeItem(value: unknown): value is GitLabTreeItem {
  return (
    isRecord(value) &&
    typeof value.id === 'string' &&
    typeof value.name === 'string' &&
    typeof value.path === 'string' &&
    (value.type === 'blob' || value.type === 'tree' || value.type === 'commit')
  );
}

export function hasGitLabSearchApi(
  value: unknown
): value is { Search: { all: (...args: unknown[]) => Promise<unknown> } } {
  return (
    isRecord(value) &&
    isRecord(value.Search) &&
    typeof value.Search.all === 'function'
  );
}

export function hasGitLabAllDiffs(value: unknown): value is {
  allDiffs: (
    projectId: number | string,
    mrIid: number,
    options?: { perPage?: number }
  ) => Promise<unknown>;
} {
  return isRecord(value) && typeof value.allDiffs === 'function';
}
