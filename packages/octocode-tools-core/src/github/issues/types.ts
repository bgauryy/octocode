import type { IssueSearchParams } from '../queryBuilders.js';

export type IssueCommentRow = {
  id: string;
  user: string;
  body: string;
  created_at: string;
  updated_at: string;
  commentType: 'discussion';
};

export type IssueRow = {
  number: number;
  title: string;
  state: string;
  author: string;
  labels: string[];
  created_at: string;
  updated_at: string;
  closed_at?: string;
  url: string;
  body?: string;
  comments?: IssueCommentRow[];
  contentPagination?: {
    body?: {
      charOffset: number;
      charLength: number;
      totalChars: number;
      hasMore: boolean;
      nextCharOffset?: number;
    };
    comments?: {
      currentPage: number;
      itemsPerPage: number;
      totalComments: number;
      hasMore: boolean;
      nextCommentPage?: number;
    };
  };
};

export type IssuesResult = {
  type: 'issues';
  owner: string;
  repo: string;
  issues: IssueRow[] | string[];
  total_count?: number;
  effectiveQuery?: string;
  incomplete_results?: boolean;
  pagination?: {
    currentPage: number;
    perPage: number;
    hasMore: boolean;
    nextPage?: number;
    totalMatches?: number;
    reportedTotalMatches?: number;
    totalMatchesKind?: 'reported';
  };
};

export type FetchIssuesParams = IssueSearchParams & {
  issueNumber?: number;
  keywordsToSearch?: string[];
  concise?: boolean;
  content?: {
    body?: boolean;
    comments?: {
      discussion?: boolean;
      includeBots?: boolean;
    };
  };
  charOffset?: number;
  charLength?: number;
  commentPage?: number;
  itemsPerPage?: number;
};
