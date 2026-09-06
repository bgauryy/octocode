import type { ProcessedBulkResult } from '../../types/toolResults.js';
import { GITHUB_GET_HISTORY_ITEM_TOOL_NAME } from '../toolNames.js';
import { GitHubGetHistoryItemQueryLocalSchema } from './historySchemes.js';

type ContentAxis =
  | 'body'
  | 'changedFiles'
  | 'comments'
  | 'commentBody'
  | 'reviews'
  | 'reviewBody'
  | 'commits'
  | 'patches'
  | 'filePaths';

const CONTENT_CONTINUATION_NAMES: Record<ContentAxis, string> = {
  body: 'continueBody',
  changedFiles: 'nextChangedFilesPage',
  comments: 'nextCommentsPage',
  commentBody: 'continueCommentBody',
  reviews: 'nextReviewsPage',
  reviewBody: 'continueReviewBody',
  commits: 'nextCommitsPage',
  patches: 'continuePatch',
  filePaths: 'nextFilePathsPage',
};

const CONTENT_CONTINUATION_CONTROLS: Record<ContentAxis, string> = {
  body: 'charOffset',
  changedFiles: 'filePage',
  comments: 'commentPage',
  commentBody: 'commentBodyOffset',
  reviews: 'reviewPage',
  reviewBody: 'charOffset',
  commits: 'commitPage',
  patches: 'charOffset',
  filePaths: 'filePage',
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

export function mergePartialReasons(value: unknown, reason: string): string[] {
  const current = Array.isArray(value)
    ? value.filter((entry): entry is string => typeof entry === 'string')
    : [];
  return [...new Set([...current, reason])];
}

function contentContinuationWhy(axis: ContentAxis): string {
  const labels: Record<ContentAxis, string> = {
    body: 'Continue the pull-request or issue body.',
    changedFiles: 'Continue the pull-request changed-file list.',
    comments: 'Continue the discussion comment list.',
    commentBody: 'Continue the current comment body.',
    reviews: 'Continue the review list.',
    reviewBody: 'Continue the current review body windows.',
    commits: 'Continue the pull-request commit list.',
    patches: 'Continue the current patch window.',
    filePaths: 'Continue the pull-request file-path list.',
  };
  return labels[axis];
}

function preservesPaginationControls(
  source: Record<string, unknown>,
  parsed: Record<string, unknown>
): boolean {
  return [
    'filePage',
    'commentPage',
    'commitPage',
    'reviewPage',
    'collectionPages',
    'charOffset',
    'commentBodyOffset',
  ].every(
    key =>
      source[key] === undefined ||
      JSON.stringify(source[key]) === JSON.stringify(parsed[key])
  );
}

/** Promote formatter-private nextQuery values to public executable calls. */
export function withContentContinuations(
  result: ProcessedBulkResult,
  operation: 'pullRequest' | 'issue'
): ProcessedBulkResult {
  const resultKey = operation === 'pullRequest' ? 'pullRequests' : 'issues';
  const rows = result[resultKey];
  if (!Array.isArray(rows) || rows.length === 0 || !isRecord(rows[0])) {
    return result;
  }
  const pagination = rows[0].contentPagination;
  if (!isRecord(pagination)) return result;

  const currentNext = isRecord(result.next) ? result.next : {};
  const next: Record<string, unknown> = { ...currentNext };
  const promotedPagination: Record<string, unknown> = {};
  let hasPartialContent = false;
  let terminalLimit = false;

  for (const [rawAxis, rawEntry] of Object.entries(pagination)) {
    if (!isRecord(rawEntry)) {
      promotedPagination[rawAxis] = rawEntry;
      continue;
    }
    const { nextQuery, ...entry } = rawEntry;
    const axis = rawAxis as ContentAxis;
    if (entry.hasMore === true && axis in CONTENT_CONTINUATION_NAMES) {
      hasPartialContent = true;
      const parsed = isRecord(nextQuery)
        ? GitHubGetHistoryItemQueryLocalSchema.safeParse(nextQuery)
        : undefined;
      const control =
        operation === 'issue' && axis === 'commentBody'
          ? 'charOffset'
          : CONTENT_CONTINUATION_CONTROLS[axis];
      const hasCursor =
        isRecord(nextQuery) && typeof nextQuery[control] === 'number';
      if (
        parsed?.success &&
        hasCursor &&
        preservesPaginationControls(
          nextQuery as Record<string, unknown>,
          parsed.data as Record<string, unknown>
        )
      ) {
        next[CONTENT_CONTINUATION_NAMES[axis]] = {
          tool: GITHUB_GET_HISTORY_ITEM_TOOL_NAME,
          query: parsed.data,
          why: contentContinuationWhy(axis),
          confidence: 'exact',
        };
      } else {
        terminalLimit = true;
        entry.continuationUnavailable = {
          reason: hasCursor ? 'schemaLimit' : 'missingCursor',
        };
      }
    }
    promotedPagination[rawAxis] = entry;
  }

  if (!hasPartialContent) return result;
  const promotedRows = [...rows];
  promotedRows[0] = {
    ...rows[0],
    contentPagination: promotedPagination,
  };
  return {
    ...result,
    [resultKey]: promotedRows,
    isPartial: true,
    partialReasons: mergePartialReasons(
      result.partialReasons,
      'contentPagination'
    ),
    ...(terminalLimit ? { terminalLimit: true } : {}),
    ...(Object.keys(next).length > 0 ? { next } : {}),
  };
}

export function withTruncatedCommitContinuation(
  result: ProcessedBulkResult,
  query: Record<string, unknown>
): ProcessedBulkResult {
  const commits = Array.isArray(result.commits) ? result.commits : [];
  const truncated = commits.find(
    commit => isRecord(commit) && commit.messageTruncated === true
  );
  if (!isRecord(truncated) || typeof truncated.sha !== 'string') return result;

  const continuationQuery = {
    operation: 'commit' as const,
    owner: query.owner,
    repo: query.repo,
    ref: truncated.sha,
  };
  const parsed =
    GitHubGetHistoryItemQueryLocalSchema.safeParse(continuationQuery);
  if (!parsed.success) return result;
  const currentNext = isRecord(result.next) ? result.next : {};
  return {
    ...result,
    isPartial: true,
    partialReasons: mergePartialReasons(
      result.partialReasons,
      'commitMessageTruncated'
    ),
    next: {
      ...currentNext,
      readCommit: {
        tool: GITHUB_GET_HISTORY_ITEM_TOOL_NAME,
        query: parsed.data,
        why: `Read the complete message for commit ${truncated.sha}.`,
        confidence: 'exact',
      },
    },
  };
}
