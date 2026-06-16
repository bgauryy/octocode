import type { PaginationInfo } from '../../types/toolResults.js';

export interface GitHubPullRequestApiItem {
  number: number;
  title: string;
  url: string;
  state: 'open' | 'closed';
  draft: boolean;
  merged: boolean;
  created_at: string;
  updated_at: string;
  closed_at?: string;
  merged_at?: string;
  author: string;
  assignees?: string[];
  labels?: Array<{
    id: number;
    name: string;
    color: string;
    description?: string;
  }>;
  head_ref: string;
  head_sha?: string;
  base_ref: string;
  base_sha?: string;
  body?: string | null;
  body_pagination?: {
    charOffset: number;
    charLength: number;
    totalChars: number;
    hasMore: boolean;
    nextCharOffset?: number;
  };
  comments?: number;
  comment_details_breakdown?: {
    inline_review: number;
    discussion: number;
  };
  commits?: number;
  additions?: number;
  deletions?: number;
  changed_files?: number;
  comment_details?: Array<{
    id: string;
    user: string;
    body: string;
    created_at: string;
    updated_at: string;
    commentType?: 'discussion' | 'review_inline';
    path?: string;
    line?: number;

    in_reply_to_id?: number | null;
    body_pagination?: {
      charOffset: number;
      charLength: number;
      totalChars: number;
      hasMore: boolean;
      nextCharOffset?: number;
    };
  }>;
  comment_details_shown?: number;
  comment_details_total?: number;
  comment_details_paginated?: boolean;
  file_changes?: Array<{
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
    submitted_at?: string;
    commit_id?: string;
  }>;
  commit_details?: Array<{
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

  _sanitization_warnings?: string[];
}

export interface GitHubPullRequestSearchApiData {
  owner?: string;
  repo?: string;
  pull_requests?: GitHubPullRequestApiItem[];
  total_count?: number;
  incomplete_results?: boolean;

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
