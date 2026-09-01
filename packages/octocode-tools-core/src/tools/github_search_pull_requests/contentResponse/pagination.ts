import {
  MAX_PAGE_NUMBER,
  PR_CONTENT_DEFAULT_ITEMS_PER_PAGE,
} from '../../../config.js';
import { publicPullRequestContinuationQuery } from '../historyContinuations.js';

export type QueryLike = {
  owner?: string;
  repo?: string;
  prNumber?: number;
  page?: number;
  filePage?: number;
  commentPage?: number;
  commitPage?: number;
  pageSize?: number;
  charOffset?: number;
  commentBodyOffset?: number;
  charLength?: number;
  matchString?: string;
  content?: Record<string, unknown>;
  type?: string;
};

export function matchStringNeedle(query: QueryLike): string | undefined {
  const raw = query.matchString;
  if (typeof raw !== 'string') return undefined;
  const trimmed = raw.trim().toLowerCase();
  return trimmed.length > 0 ? trimmed : undefined;
}

export function containsNeedle(value: unknown, needle: string): boolean {
  return typeof value === 'string' && value.toLowerCase().includes(needle);
}

export type Pagination = {
  currentPage: number;
  totalPages: number;
  itemsPerPage: number;
  totalItems: number;
  hasMore: boolean;
  nextPage?: number;
  terminalLimit?: boolean;
  continuationUnavailable?: {
    reason: 'schemaPageLimit';
    maxPage: number;
  };
};

export type TextPagination = {
  charOffset: number;
  charLength: number;
  totalChars: number;
  hasMore: boolean;
  nextCharOffset?: number;
};

export type ContentPaginationEntry = Record<string, unknown> & {
  hasMore: boolean;
  nextQuery?: Record<string, unknown>;
};

export type ContentPagination = Partial<
  Record<
    | 'body'
    | 'changedFiles'
    | 'comments'
    | 'commentBody'
    | 'commits'
    | 'patches'
    | 'filePaths',
    ContentPaginationEntry
  >
>;

export function paginateItems<T>(
  items: T[],
  page = 1,
  itemsPerPage = PR_CONTENT_DEFAULT_ITEMS_PER_PAGE
): {
  items: T[];
  pagination: Pagination;
} {
  const safePerPage = Math.min(Math.max(1, itemsPerPage), 100);
  const totalItems = items.length;
  const totalPages = Math.max(1, Math.ceil(totalItems / safePerPage));
  const currentPage = Math.min(Math.max(1, page), totalPages);
  const start = (currentPage - 1) * safePerPage;
  const end = Math.min(start + safePerPage, totalItems);
  return {
    items: items.slice(start, end),
    pagination: {
      currentPage,
      totalPages,
      itemsPerPage: safePerPage,
      totalItems,
      hasMore: currentPage < totalPages,
      ...(currentPage < totalPages && currentPage < MAX_PAGE_NUMBER
        ? { nextPage: currentPage + 1 }
        : {}),
      ...(currentPage < totalPages && currentPage >= MAX_PAGE_NUMBER
        ? {
            terminalLimit: true,
            continuationUnavailable: {
              reason: 'schemaPageLimit' as const,
              maxPage: MAX_PAGE_NUMBER,
            },
          }
        : {}),
    },
  };
}

export function paginateText(
  value: string | undefined,
  charOffset = 0,
  charLength = 12_000
) {
  if (typeof value !== 'string') return undefined;
  const totalChars = value.length;
  const start = Math.min(Math.max(0, charOffset), totalChars);
  const length = Math.min(Math.max(1, charLength), 50_000);
  const end = Math.min(start + length, totalChars);
  const hasMore = end < totalChars;
  return {
    content: value.slice(start, end),
    pagination: {
      charOffset: start,
      charLength: end - start,
      totalChars,
      hasMore,
      ...(hasMore ? { nextCharOffset: end } : {}),
    },
  };
}

export function compactBody(value: unknown, max = 500): string | undefined {
  if (typeof value !== 'string' || value.length === 0) return undefined;
  return value.length <= max ? value : `${value.slice(0, max - 3)}...`;
}

export function baseQuery(query: QueryLike, prNumber: number) {
  return {
    operation: 'pullRequest',
    owner: query.owner,
    repo: query.repo,
    number: prNumber,
  };
}

export function continuationQuery(
  query: QueryLike,
  prNumber: number,
  patch: Record<string, unknown>
): Record<string, unknown> {
  return publicPullRequestContinuationQuery(
    query as Record<string, unknown>,
    prNumber,
    patch
  );
}

export function textContinuationQuery(
  query: QueryLike,
  prNumber: number,
  content: Record<string, unknown>,
  pagination: TextPagination,
  extra: Record<string, unknown> = {}
): Record<string, unknown> | undefined {
  if (!pagination.hasMore || pagination.nextCharOffset === undefined) {
    return undefined;
  }

  return continuationQuery(query, prNumber, {
    content,
    ...extra,
    charOffset: pagination.nextCharOffset,
    charLength: query.charLength,
  });
}

export function pageContinuationQuery(
  query: QueryLike,
  prNumber: number,
  content: Record<string, unknown>,
  pageKey: 'filePage' | 'commentPage' | 'commitPage',
  pagination: Pagination
): Record<string, unknown> | undefined {
  if (!pagination.hasMore || pagination.nextPage === undefined) {
    return undefined;
  }

  return continuationQuery(query, prNumber, {
    content,
    [pageKey]: pagination.nextPage,
    pageSize: query.pageSize,
  });
}

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

export function readPagination(value: unknown): Pagination | undefined {
  if (!isRecord(value)) return undefined;
  if (
    typeof value.currentPage !== 'number' ||
    typeof value.totalPages !== 'number' ||
    typeof value.itemsPerPage !== 'number' ||
    typeof value.totalItems !== 'number' ||
    typeof value.hasMore !== 'boolean'
  ) {
    return undefined;
  }

  return value as Pagination;
}

export function readTextPagination(value: unknown): TextPagination | undefined {
  if (!isRecord(value)) return undefined;
  if (
    typeof value.charOffset !== 'number' ||
    typeof value.charLength !== 'number' ||
    typeof value.totalChars !== 'number' ||
    typeof value.hasMore !== 'boolean'
  ) {
    return undefined;
  }

  return value as TextPagination;
}
