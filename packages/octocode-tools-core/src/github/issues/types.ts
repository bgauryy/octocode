import type { IssueSearchParams } from '../queryBuilders/issues.js';

export type IssueCommentRow = {
  id: string;
  user: string;
  body: string;
  bodyPagination?: NonNullable<IssueRow['contentPagination']>['body'];
  createdAt: string;
  updatedAt: string;
  commentType: 'discussion';
};

export type IssueRow = {
  number: number;
  title: string;
  state: string;
  author: string;
  labels: string[];
  createdAt: string;
  updatedAt: string;
  closedAt?: string;
  body?: string;
  comments?: IssueCommentRow[];
  contentPagination?: {
    commentBody?: NonNullable<IssueRow['contentPagination']>['body'];
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
      terminalLimit?: boolean;
      continuationUnavailable?: {
        reason: 'schemaPageLimit';
        maxPage: number;
      };
    };
  };
};

export type IssuesResult = {
  type: 'issues';
  owner: string;
  repo: string;
  issues: IssueRow[] | string[];
  totalCount?: number;
  effectiveQuery?: string;
  warnings?: string[];
  incompleteResults?: boolean;
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
