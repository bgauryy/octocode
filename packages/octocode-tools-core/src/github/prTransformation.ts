import {
  GitHubPullRequestsSearchParams,
  GitHubPullRequestItem,
  DiffEntry,
  CommitFileInfo,
} from './githubAPI.js';
import { ContentSanitizer } from 'octocode-security/contentSanitizer';
import { filterPatch, trimDiffContext } from '../utils/parsers/diff.js';
import { contextUtils } from '../utils/contextUtils.js';

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
  head?: { ref?: string; sha?: string };
  base?: { ref?: string; sha?: string };

  comments?: number | null;
}

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
    ...(typeof item.comments === 'number' && item.comments > 0
      ? { total_comment_count: item.comments }
      : {}),
    reactions: 0,
    draft: item.draft ?? false,
    head: item.head?.ref,
    head_sha: item.head?.sha,
    base: item.base?.ref,
    base_sha: item.base?.sha,
    ...(item.merged_at && { merged_at: item.merged_at }),
  };

  return { prData, sanitizationWarnings };
}

import { getOutputCharLimit } from '../utils/pagination/charLimit.js';
const SEARCH_RESULT_BODY_CHAR_LENGTH = getOutputCharLimit();
const SEARCH_RESULT_COMMENT_BODY_CHAR_LENGTH = Math.round(
  getOutputCharLimit() / 4
);
const SEARCH_RESULT_MAX_COMMENT_DETAILS = 3;

interface PRResponseFormatOptions {
  includeFullBody?: boolean;
  includeFullCommentDetails?: boolean;
  charOffset?: number;
  charLength?: number;
}

interface TextPaginationInfo {
  charOffset: number;
  charLength: number;
  totalChars: number;
  hasMore: boolean;
  nextCharOffset?: number;
}

function paginateText(
  value: string | null | undefined,
  charOffset: number,
  charLength: number,
  enabled: boolean
): { value: string | null | undefined; pagination?: TextPaginationInfo } {
  if (typeof value !== 'string' || !enabled) {
    return { value };
  }

  const totalChars = value.length;
  const safeOffset = Math.min(Math.max(0, charOffset), totalChars);
  const safeLength = Math.max(1, charLength);
  const endOffset = Math.min(safeOffset + safeLength, totalChars);
  const hasMore = endOffset < totalChars;

  if (safeOffset === 0 && endOffset === totalChars) {
    return { value };
  }

  return {
    value: value.slice(safeOffset, endOffset),
    pagination: {
      charOffset: safeOffset,
      charLength: endOffset - safeOffset,
      totalChars,
      hasMore,
      ...(hasMore ? { nextCharOffset: endOffset } : {}),
    },
  };
}

export function formatPRForResponse(
  pr: GitHubPullRequestItem,
  options: PRResponseFormatOptions = {}
) {
  const charOffset = options.charOffset ?? 0;
  const bodyCharLength = options.charLength ?? SEARCH_RESULT_BODY_CHAR_LENGTH;
  const commentCharLength =
    options.charLength ?? SEARCH_RESULT_COMMENT_BODY_CHAR_LENGTH;
  const rawBody =
    typeof pr.body === 'string'
      ? contextUtils.minifyMarkdownCore(pr.body)
      : pr.body;
  const body = paginateText(
    rawBody,
    charOffset,
    bodyCharLength,
    !options.includeFullBody
  );
  const comments = (pr.comments ?? []).sort((a, b) => {
    const aIsInline = a.commentType === 'review_inline' ? 0 : 1;
    const bIsInline = b.commentType === 'review_inline' ? 0 : 1;
    return aIsInline - bIsInline;
  });

  const inlineReviewCount = comments.filter(
    c => c.commentType === 'review_inline'
  ).length;
  const discussionCount = comments.length - inlineReviewCount;

  const visibleComments = options.includeFullCommentDetails
    ? comments
    : comments.slice(0, SEARCH_RESULT_MAX_COMMENT_DETAILS);
  const commentDetails = visibleComments.map(comment => {
    const bodyResult = paginateText(
      comment.body,
      charOffset,
      commentCharLength,
      !options.includeFullCommentDetails
    );

    return {
      ...comment,
      body: bodyResult.value ?? '',
      ...(bodyResult.pagination
        ? { body_pagination: bodyResult.pagination }
        : {}),
    };
  });
  const commentDetailsPaginated =
    !options.includeFullCommentDetails &&
    (comments.length > visibleComments.length ||
      commentDetails.some(comment => 'body_pagination' in comment));
  const paginationWarnings = [
    ...(body.pagination && options.includeFullBody
      ? [
          `PR body paginated at charOffset=${body.pagination.charOffset}, charLength=${body.pagination.charLength}, totalChars=${body.pagination.totalChars}. Re-call with prNumber and charOffset=${body.pagination.nextCharOffset ?? body.pagination.totalChars} to continue this body.`,
        ]
      : []),
    ...(commentDetailsPaginated
      ? [
          `PR comments are paginated/summarized to ${SEARCH_RESULT_MAX_COMMENT_DETAILS} comment(s) per search result with ${commentCharLength} chars each. Use prNumber with content.comments={discussion:true,reviewInline:true} and charOffset to continue specific comment bodies.`,
        ]
      : []),
  ];

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
    ...(pr.labels?.length
      ? { labels: pr.labels.map(name => ({ id: 0, name, color: '' })) }
      : {}),
    head_ref: pr.head || '',
    ...(pr.head_sha ? { head_sha: pr.head_sha } : {}),
    base_ref: pr.base || '',
    ...(pr.base_sha ? { base_sha: pr.base_sha } : {}),
    body: body.value,
    ...(body.pagination ? { body_pagination: body.pagination } : {}),
    comments: pr.total_comment_count ?? comments.length,
    ...(comments.length > 0 && {
      comment_details_breakdown: {
        inline_review: inlineReviewCount,
        discussion: discussionCount,
      },
    }),
    commits: pr.commits?.length || 0,
    additions:
      pr.file_changes?.files.reduce((sum, file) => sum + file.additions, 0) ||
      pr.additions ||
      0,
    deletions:
      pr.file_changes?.files.reduce((sum, file) => sum + file.deletions, 0) ||
      pr.deletions ||
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
    ...(pr.reviews && {
      reviews: pr.reviews,
    }),
    ...(pr.commits && {
      commit_details: pr.commits,
    }),
    ...(commentDetails.length > 0 && {
      comment_details: commentDetails,
      comment_details_shown: commentDetails.length,
      comment_details_total: comments.length,
      ...(commentDetailsPaginated ? { comment_details_paginated: true } : {}),
    }),
    ...((pr._sanitization_warnings || paginationWarnings.length > 0) && {
      _sanitization_warnings: [
        ...(pr._sanitization_warnings || []),
        ...paginationWarnings,
      ],
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
  const content = params.content as
    | {
        patches?: {
          mode?: 'none' | 'selected' | 'all';
          files?: string[];
          ranges?: Array<{
            file: string;
            additions?: number[];
            deletions?: number[];
          }>;
        };
      }
    | undefined;
  const patches = content?.patches;
  const mode = patches?.mode ?? (params.reviewMode === 'full' ? 'all' : 'none');
  const metadataMap = new Map(
    patches?.ranges?.map(range => [range.file, range]) || []
  );
  const selectedFiles = new Set([
    ...(patches?.files ?? []),
    ...metadataMap.keys(),
  ]);

  if (mode === 'none') {
    return files.map(file => ({ ...file, patch: undefined }));
  }

  if (mode === 'selected') {
    return files
      .filter(file => selectedFiles.has(file.filename))
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

  return files.map(file => ({
    ...file,
    patch: file.patch ? trimDiffContext(file.patch) : file.patch,
  }));
}
