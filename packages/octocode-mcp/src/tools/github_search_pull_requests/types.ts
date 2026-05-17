import type { GitHubSearchPullRequestsPagination } from '@octocodeai/octocode-core';

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
  comments?: number;
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
  }>;
  file_changes?: Array<{
    filename: string;
    status: string;
    additions: number;
    deletions: number;
    changes?: number;
    patch?: string;
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
}

export interface GitHubPullRequestSearchApiData {
  owner?: string;
  repo?: string;
  pull_requests?: GitHubPullRequestApiItem[];
  total_count?: number;
  incomplete_results?: boolean;
  pagination?: GitHubSearchPullRequestsPagination;
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
