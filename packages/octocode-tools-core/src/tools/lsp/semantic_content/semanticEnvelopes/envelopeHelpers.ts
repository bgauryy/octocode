import {
  compactResolvedSymbol,
  type LspSearchQuery,
  type LspSemanticEnvelope,
  type SemanticEmptyCategory,
  type SemanticContentType,
} from '../../shared/semanticTypes.js';
import type { SymbolAnchor } from '../../shared/resolveSymbolAnchor.js';
import { MAX_PAGE_NUMBER } from '@octocodeai/octocode-core/schema';
import { semanticSnapshotItems } from '../semanticSnapshot.js';

export const DEFAULT_SYMBOLS_PER_PAGE = 40;
export const DEFAULT_LOCATIONS_PER_PAGE = 40;
export const DEFAULT_CALLS_PER_PAGE = 10;

export type PaginationInfo = {
  currentPage: number;
  totalPages: number;
  totalResults: number;
  hasMore: boolean;
  pageSize: number;
  nextPage?: number;
  snapshot?: string;
};

export function failedAnchorEnvelope(
  query: LspSearchQuery,
  reason: string,
  category: 'anchorFailed' | 'symbolNotFound'
): LspSemanticEnvelope {
  const uri = query.uri ?? '';
  return {
    type: query.operation,
    uri,
    lsp: {},
    payload: {
      kind: 'empty',
      category,
      reason,
    },
  };
}

export function emptyEnvelope(
  type: SemanticContentType,
  anchor: SymbolAnchor,
  reason: string,
  category: SemanticEmptyCategory,
  serverAvailable = false
): LspSemanticEnvelope {
  return {
    type,
    uri: anchor.uri,
    resolvedSymbol: compactResolvedSymbol(anchor.resolvedSymbol),
    lsp: { serverAvailable },
    payload: {
      kind: 'empty',
      category,
      reason,
    },
  };
}

export function paginateItems<T>(
  items: readonly T[],
  requestedPage: number,
  requestedItemsPerPage: number,
  query?: LspSearchQuery,
  identities?: readonly unknown[]
): { pageItems: T[]; pagination: PaginationInfo } {
  const snapshot = query
    ? semanticSnapshotItems(items, query, identities)
    : undefined;
  items = snapshot?.items ?? items;
  const itemsPerPage = Math.max(1, requestedItemsPerPage);
  const totalPages = Math.max(1, Math.ceil(items.length / itemsPerPage));
  const currentPage = Math.min(Math.max(1, requestedPage), totalPages);
  const start = (currentPage - 1) * itemsPerPage;
  const pageItems = items.slice(start, start + itemsPerPage);
  const hasMore = currentPage < totalPages;

  return {
    pageItems,
    pagination: {
      currentPage,
      totalPages,
      totalResults: items.length,
      hasMore,
      pageSize: itemsPerPage,
      ...(snapshot ? { snapshot: snapshot.snapshot } : {}),
      ...(hasMore && currentPage < MAX_PAGE_NUMBER
        ? { nextPage: currentPage + 1 }
        : {}),
    },
  };
}
