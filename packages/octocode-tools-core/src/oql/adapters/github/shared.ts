/**
 * GitHub provider adapter — shared types and pure helpers used by the
 * per-lane execute functions (code/files/content/structure). Kept separate
 * from provider-error/status classification (see ./provider-diagnostics.ts).
 */
import type { CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type { GitHubFileContentData } from '@octocodeai/octocode-core/types';
import type { ToolPaginationPayload } from '../pagination.js';
import type { OqlQuery, OqlTreeResultRow, QuerySource } from '../../types.js';

export type GitHubContentPagination = {
  currentPage?: number;
  totalPages?: number;
  hasMore?: boolean;
  charOffset?: number;
  charLength?: number;
  totalChars?: number;
};

export type GitHubContentRow = GitHubFileContentData & {
  pagination?: GitHubContentPagination;
};

// NOTE: this adapter's splitRepo intentionally drops a bare (slash-less) repo on
// the else-branch, unlike transformers/github/common.splitGithubSource which
// keeps it — do not "dedupe" them without reconciling that difference first.
export function splitRepo(source: QuerySource | undefined): {
  owner?: string;
  repo?: string;
} {
  if (source?.kind !== 'github') return {};
  if (source.repo && source.repo.includes('/')) {
    const [owner, repo] = source.repo.split('/');
    return { owner, repo };
  }
  return { owner: source.owner };
}

/** Pull the single query's `data` payload from a bulk CallToolResult. */
export function extractData<T>(result: CallToolResult): T | undefined {
  const sc = result.structuredContent as
    | { results?: Array<{ data?: unknown } | Record<string, unknown>> }
    | undefined;
  const first = sc?.results?.[0];
  if (!first) return undefined;
  return ('data' in first ? first.data : first) as T | undefined;
}

export function extractStatus(result: CallToolResult): string | undefined {
  const sc = result.structuredContent as
    { results?: Array<{ status?: string }> } | undefined;
  return sc?.results?.[0]?.status;
}

export interface GithubStructureEntry {
  dir?: string;
  files?: readonly string[];
  folders?: readonly string[];
}

export interface GithubCodeSearchMatch {
  value?: string;
  matchIndices?: Array<{ start: number; end: number; lineOffset?: number }>;
}

export interface GithubCodeSearchFile {
  owner?: string;
  repo?: string;
  queryId?: string;
  path: string;
  matches?: readonly GithubCodeSearchMatch[];
}

export interface GithubCodeSearchPayload {
  files?: readonly (GithubCodeSearchFile | string)[];
  pagination?: ToolPaginationPayload;
}

export function cleanRepoPath(part: string | undefined): string {
  if (!part || part === '.') return '';
  return part.replace(/^\/+|\/+$/g, '');
}

export function joinRepoPath(...parts: Array<string | undefined>): string {
  return parts.map(cleanRepoPath).filter(Boolean).join('/');
}

export function normalizeStructure(
  structure:
    | readonly GithubStructureEntry[]
    | Record<string, { files?: readonly string[]; folders?: readonly string[] }>
    | undefined
): GithubStructureEntry[] {
  if (!structure) return [];
  if (Array.isArray(structure)) return [...structure];
  return Object.entries(structure).map(([dir, entry]) => ({
    dir,
    files: entry.files,
    folders: entry.folders,
  }));
}

export function structureDepth(pathValue: string): number {
  return cleanRepoPath(pathValue).split('/').filter(Boolean).length;
}

export function normalizeExtension(value: string): string {
  return value.trim().toLowerCase().replace(/^\*\./, '').replace(/^\./, '');
}

export function fileExtension(pathValue: string): string {
  const base = pathValue.split('/').pop() ?? pathValue;
  const dot = base.lastIndexOf('.');
  return dot > 0 ? base.slice(dot + 1).toLowerCase() : '';
}

export function escapeRegex(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

export function treePatternMatches(
  pathValue: string,
  pattern: string
): boolean {
  const normalized = pattern.trim();
  if (!normalized) return true;
  const base = pathValue.split('/').pop() ?? pathValue;
  if (!normalized.includes('*') && !normalized.includes('?')) {
    return base.includes(normalized) || pathValue.includes(normalized);
  }
  const expression = normalized
    .split('**')
    .map(part =>
      part
        .split('*')
        .map(segment => segment.split('?').map(escapeRegex).join('[^/]'))
        .join('[^/]*')
    )
    .join('.*');
  const matcher = new RegExp(`^${expression}$`);
  return matcher.test(base) || matcher.test(pathValue);
}

export function filterGithubTreeRows(
  rows: readonly OqlTreeResultRow[],
  query: OqlQuery
): OqlTreeResultRow[] {
  const tree = query.fetch?.tree;
  if (!tree) return [...rows];
  const extensions = (tree.extensions ?? [])
    .map(normalizeExtension)
    .filter(Boolean);

  return rows.filter(row => {
    if (tree.filesOnly && row.entryType !== 'file') return false;
    if (tree.directoriesOnly && row.entryType !== 'directory') return false;
    if (tree.pattern && !treePatternMatches(row.path, tree.pattern)) {
      return false;
    }
    return (
      row.entryType === 'directory' ||
      extensions.length === 0 ||
      extensions.includes(fileExtension(row.path))
    );
  });
}

export function githubCodeFilePath(
  file: GithubCodeSearchFile | string
): string {
  if (typeof file !== 'string') return file.path;
  const separator = file.indexOf(':');
  return separator >= 0 ? file.slice(separator + 1) : file;
}

export function githubCodeFileMetadata(
  file: GithubCodeSearchFile | string
): Record<string, unknown> | undefined {
  if (typeof file === 'string') return undefined;
  const metadata = {
    ...(file.owner !== undefined ? { owner: file.owner } : {}),
    ...(file.repo !== undefined ? { repo: file.repo } : {}),
    ...(file.queryId !== undefined ? { queryId: file.queryId } : {}),
  };
  return Object.keys(metadata).length ? metadata : undefined;
}

export function githubCodeFileMatches(
  file: GithubCodeSearchFile | string
): readonly GithubCodeSearchMatch[] {
  return typeof file === 'string' ? [] : (file.matches ?? []);
}

/** GitHub source, guaranteed by dispatch. */
export type GithubSource = Extract<QuerySource, { kind: 'github' }>;
export function ghFrom(query: OqlQuery): GithubSource {
  return (query.from ?? { kind: 'github' }) as GithubSource;
}

export function normalizeContentRange(
  range: NonNullable<NonNullable<OqlQuery['fetch']>['content']>['range']
): { startLine?: number; endLine?: number } {
  if (range?.startLine === undefined) return {};
  const contextLines = range.contextLines ?? 0;
  const startLine = Math.max(1, range.startLine - contextLines);
  const endLine = (range.endLine ?? range.startLine) + contextLines;
  return { startLine, endLine };
}
