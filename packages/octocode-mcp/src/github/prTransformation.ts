/**
 * PR transformation and formatting — converts raw GitHub API data to unified output format.
 * Extracted from pullRequestSearch.ts.
 */
import {
  GitHubPullRequestsSearchParams,
  GitHubPullRequestItem,
  DiffEntry,
  CommitFileInfo,
} from './githubAPI.js';
import { ContentSanitizer } from 'octocode-security-utils/contentSanitizer';
import { filterPatch } from '../utils/parsers/diff.js';

/**
 * Common interface for raw PR data from either Search API or REST API.
 * This enables a unified transformation function that handles both sources.
 */
interface RawPRData {
  number: number;
  title?: string | null;
  body?: string | null;
  state?: string | null;
  user?: { login: string } | null;
  labels?: (string | { name?: string | null })[] | null;
  created_at?: string | null;
  updated_at?: string | null;
  closed_at?: string | null;
  html_url: string;
  draft?: boolean | null;
  merged_at?: string | null;
  // REST-specific fields (optional - not present in Search API)
  head?: { ref?: string; sha?: string };
  base?: { ref?: string; sha?: string };
}

/**
 * Unified PR transformation function that handles both Search API and REST API responses.
 * The head/base refs will be undefined for Search API items (fetched separately from PR details).
 */
export function createBasePRTransformation(item: RawPRData): {
  prData: GitHubPullRequestItem;
  sanitizationWarnings: Set<string>;
} {
  const titleSanitized = ContentSanitizer.sanitizeContent(item.title ?? '');
  const bodySanitized = item.body
    ? ContentSanitizer.sanitizeContent(item.body)
    : { content: undefined, warnings: [] };

  const sanitizationWarnings = new Set<string>([
    ...titleSanitized.warnings,
    ...bodySanitized.warnings,
  ]);

  // GitHub PRs can only be 'open' or 'closed'. Default to 'open' if undefined.
  const normalizedState = item.state?.toLowerCase();
  const validState: 'open' | 'closed' =
    normalizedState === 'closed' ? 'closed' : 'open';

  const prData: GitHubPullRequestItem = {
    number: item.number,
    title: titleSanitized.content,
    body: bodySanitized.content,
    state: validState,
    author: item.user?.login ?? '',
    labels:
      item.labels?.map(l => (typeof l === 'string' ? l : (l.name ?? ''))) ?? [],
    created_at: item.created_at ?? '',
    updated_at: item.updated_at ?? '',
    closed_at: item.closed_at ?? null,
    url: item.html_url,
    comments: [],
    reactions: 0,
    draft: item.draft ?? false,
    // Include head/base if available (REST API), undefined otherwise (Search API)
    head: item.head?.ref,
    head_sha: item.head?.sha,
    base: item.base?.ref,
    base_sha: item.base?.sha,
    ...(item.merged_at && { merged_at: item.merged_at }),
  };

  return { prData, sanitizationWarnings };
}

/**
 * Format a transformed PR item for API response output.
 * Standardizes the output format across all PR search/fetch methods.
 */
export function formatPRForResponse(pr: GitHubPullRequestItem) {
  return {
    number: pr.number,
    title: pr.title,
    url: pr.url,
    state: pr.state as 'open' | 'closed',
    draft: pr.draft ?? false,
    merged: pr.state === 'closed' && !!pr.merged_at,
    created_at: pr.created_at,
    updated_at: pr.updated_at,
    closed_at: pr.closed_at ?? undefined,
    merged_at: pr.merged_at,
    author: pr.author,
    head_ref: pr.head || '',
    ...(pr.head_sha ? { head_sha: pr.head_sha } : {}),
    base_ref: pr.base || '',
    ...(pr.base_sha ? { base_sha: pr.base_sha } : {}),
    body: pr.body,
    comments: pr.comments?.length || 0,
    commits: pr.commits?.length || 0,
    additions:
      pr.file_changes?.files.reduce((sum, file) => sum + file.additions, 0) ||
      0,
    deletions:
      pr.file_changes?.files.reduce((sum, file) => sum + file.deletions, 0) ||
      0,
    changed_files: pr.file_changes?.total_count || 0,
    ...(pr.file_changes && {
      file_changes: pr.file_changes.files?.map(file => ({
        filename: file.filename,
        status: file.status,
        additions: file.additions,
        deletions: file.deletions,
        patch: file.patch,
      })),
    }),
    ...(pr.commits && {
      commit_details: pr.commits,
    }),
    ...(pr.comments &&
      pr.comments.length > 0 && {
        comment_details: pr.comments,
      }),
    ...(pr._sanitization_warnings && {
      _sanitization_warnings: pr._sanitization_warnings,
    }),
  };
}

export function normalizeOwnerRepo(params: GitHubPullRequestsSearchParams): {
  owner: string | undefined;
  repo: string | undefined;
} {
  const owner = Array.isArray(params.owner)
    ? params.owner[0] || undefined
    : params.owner;
  const repo = Array.isArray(params.repo)
    ? params.repo[0] || undefined
    : params.repo;
  return { owner, repo };
}

export function applyPartialContentFilter(
  files: (DiffEntry | CommitFileInfo)[],
  params: GitHubPullRequestsSearchParams
): (DiffEntry | CommitFileInfo)[] {
  const type = params.type || 'metadata';
  const metadataMap = new Map(
    params.partialContentMetadata?.map(m => [m.file, m]) || []
  );

  if (type === 'metadata') {
    return files.map(file => ({
      ...file,
      patch: undefined,
    }));
  } else if (type === 'partialContent') {
    return files
      .filter(file => metadataMap.has(file.filename))
      .map(file => {
        const meta = metadataMap.get(file.filename);
        return {
          ...file,
          patch: file.patch
            ? filterPatch(file.patch, meta?.additions, meta?.deletions)
            : undefined,
        };
      });
  }
  return files;
}
