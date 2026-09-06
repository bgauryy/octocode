import {
  compactResolvedSymbol,
  type LspGetSemanticsQuery,
  type LspSemanticEnvelope,
  type SemanticEmptyCategory,
  type SemanticContentType,
} from '../../shared/semanticTypes.js';
import type { SymbolAnchor } from '../../shared/resolveSymbolAnchor.js';
import { MAX_PAGE_NUMBER } from '../../../../config.js';
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

export function emptyCategoryForReason(
  type: SemanticContentType,
  reason: string
): SemanticEmptyCategory {
  // "unavailable" is no longer an empty category — no server now throws
  // (errorCode lspServerUnavailable) rather than returning an empty envelope.
  if (/unsupported/i.test(reason)) return 'unsupportedOperation';
  if (/could not find symbol|symbol.*not found/i.test(reason)) {
    return 'symbolNotFound';
  }
  if (/call/i.test(reason)) return 'noCalls';
  if (type === 'references') return 'noReferences';
  if (type === 'hover') return 'noHover';
  if (type === 'documentSymbols') return 'anchorFailed';
  return 'noLocations';
}

export function failedAnchorEnvelope(
  query: LspGetSemanticsQuery,
  reason: string
): LspSemanticEnvelope {
  const uri = query.uri ?? '';
  return {
    type: query.type,
    uri,
    lsp: {},
    payload: {
      kind: 'empty',
      category: emptyCategoryForReason(query.type, reason),
      reason,
    },
  };
}

export function emptyEnvelope(
  type: SemanticContentType,
  anchor: SymbolAnchor,
  reason: string,
  serverAvailable = false
): LspSemanticEnvelope {
  return {
    type,
    uri: anchor.uri,
    resolvedSymbol: compactResolvedSymbol(anchor.resolvedSymbol),
    lsp: { serverAvailable },
    payload: {
      kind: 'empty',
      category: emptyCategoryForReason(type, reason),
      reason,
    },
  };
}

export function paginateItems<T>(
  items: readonly T[],
  requestedPage: number,
  requestedItemsPerPage: number,
  query?: LspGetSemanticsQuery,
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
