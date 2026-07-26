import type { PaginationInfo } from '../../types/toolResults.js';

export interface GitHubPullRequestApiItem {
  number: number;
  title: string;
  state: 'open' | 'closed';
  draft: boolean;
  merged: boolean;
  createdAt: string;
  updatedAt: string;
  closedAt?: string;
  mergedAt?: string;
  author: string;
  assignees?: string[];
  labels?: Array<{
    id: number;
    name: string;
    color: string;
    description?: string;
  }>;
  headRef: string;
  headSha?: string;
  baseRef: string;
  baseSha?: string;
  body?: string | null;
  bodyPagination?: {
    charOffset: number;
    charLength: number;
    totalChars: number;
    hasMore: boolean;
    nextCharOffset?: number;
  };
  comments?: number;
  commentDetailsBreakdown?: {
    inlineReview: number;
    discussion: number;
  };
  commits?: number;
  additions?: number;
  deletions?: number;
  changedFiles?: number;
  commentDetails?: Array<{
    id: string;
    user: string;
    body: string;
    createdAt: string;
    updatedAt: string;
    commentType?: 'discussion' | 'review_inline';
    path?: string;
    line?: number;

    inReplyToId?: number | null;
    bodyPagination?: {
      charOffset: number;
      charLength: number;
      totalChars: number;
      hasMore: boolean;
      nextCharOffset?: number;
    };
  }>;
  commentDetailsShown?: number;
  commentDetailsTotal?: number;
  commentDetailsPaginated?: boolean;
  fileChanges?: Array<{
    filename: string;
    status: string;
    additions: number;
    deletions: number;
    changes?: number;
    patch?: string;
  }>;
  reviews?: Array<{
    id: string;
    user: string;
    state: string;
    body: string;
    submittedAt?: string;
    commitId?: string;
  }>;
  commitDetails?: Array<{
    sha: string;
    message: string;
    author: string;
    date: string;
    files: Array<{
      filename: string;
      status: string;
      additions: number;
      deletions: number;
      changes?: number;
      patch?: string;
    }>;
  }>;

  sanitizationWarnings?: string[];
}

export interface GitHubPullRequestSearchApiData {
  owner?: string;
  repo?: string;
  pullRequests?: GitHubPullRequestApiItem[];
  totalCount?: number;
  incompleteResults?: boolean;

  effectiveQuery?: string;
  pagination?: PaginationInfo;
  outputPagination?: {
    charOffset: number;
    charLength: number;
    totalChars: number;
    hasMore: boolean;
    currentPage: number;
    totalPages: number;
  };
}

export interface GitHubPullRequestSearchApiResult extends GitHubPullRequestSearchApiData {
  error?: string;
  status?: number;
  hints?: string[];
  rateLimitRemaining?: number;
  rateLimitReset?: number;
  retryAfter?: number;
  rawResponseChars?: number;
}
