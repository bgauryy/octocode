import { MAX_PAGE_NUMBER } from '../../config.js';

type PackagePageQuery = {
  keywords?: string[];
  page?: number;
  pageSize?: number;
};

export type PackagePagination = {
  currentPage: number;
  totalPages: number;
  perPage: number;
  totalFound: number;
  returned: number;
  hasMore: boolean;
  nextPage?: number;
  continuationUnavailable?: {
    reason: 'schemaPageLimit';
    maxPage: number;
  };
};

export function buildPackagePagination(
  query: PackagePageQuery,
  totalFound: number,
  returned: number,
  isKeyword: boolean
): PackagePagination {
  const currentPage = Math.max(1, query.page ?? 1);
  const perPage = isKeyword ? (query.pageSize ?? 10) : 1;

  // The keyword CLI search path reports `totalFound` as the count returned on
  // this page, not necessarily the registry grand total. A full page therefore
  // remains pageable; a partial page is terminal.
  const pageIsFull = isKeyword && returned >= perPage;
  const totalPagesFromCount = Math.max(1, Math.ceil(totalFound / perPage));
  const hasMore = pageIsFull || currentPage < totalPagesFromCount;
  const totalPages =
    pageIsFull && currentPage >= totalPagesFromCount
      ? currentPage + 1
      : totalPagesFromCount;

  return {
    currentPage,
    totalPages,
    perPage,
    totalFound,
    returned,
    hasMore,
    ...(hasMore && currentPage < MAX_PAGE_NUMBER
      ? { nextPage: currentPage + 1 }
      : {}),
    ...(hasMore && currentPage >= MAX_PAGE_NUMBER
      ? {
          continuationUnavailable: {
            reason: 'schemaPageLimit' as const,
            maxPage: MAX_PAGE_NUMBER,
          },
        }
      : {}),
  };
}

export function buildPackagePageContinuation(
  query: PackagePageQuery,
  pagination: PackagePagination
): Record<string, unknown> | undefined {
  if (
    !Array.isArray(query.keywords) ||
    typeof pagination.nextPage !== 'number'
  ) {
    return undefined;
  }
  return {
    nextPage: {
      tool: 'npmSearch',
      query: {
        keywords: query.keywords,
        page: pagination.nextPage,
        ...(typeof query.pageSize === 'number'
          ? { pageSize: query.pageSize }
          : {}),
      },
      why: `Continue npm keyword discovery on page ${pagination.nextPage}.`,
      confidence: 'exact',
    },
  };
}
