import { acquirePooledClient } from '@octocodeai/octocode-engine/lsp/manager';
import type {
  CodeSnippet,
  LSPRange,
  OutgoingCall,
  ReferenceLocation,
  ReferencesByFile,
} from '@octocodeai/octocode-engine/lsp/types';
import {
  gatherIncomingCallsRecursive,
  gatherOutgoingCallsRecursive,
  createCallItemKey,
} from '../shared/callHierarchyTraversal.js';
import {
  compactLocation,
  compactResolvedSymbol,
  type LspGetSemanticsQuery,
  type LspSemanticEnvelope,
  type SemanticEmptyCategory,
  type SemanticContentType,
  type SymbolAnchoredSemanticQuery,
} from '../shared/semanticTypes.js';
import type { SymbolAnchor } from '../shared/resolveSymbolAnchor.js';
import {
  compactCallItem,
  compactIncomingCall,
  compactOutgoingCall,
} from './semanticPresentation.js';

export const DEFAULT_SYMBOLS_PER_PAGE = 40;
export const DEFAULT_LOCATIONS_PER_PAGE = 40;
export const DEFAULT_CALLS_PER_PAGE = 10;

export type PaginationInfo = {
  currentPage: number;
  totalPages: number;
  totalResults: number;
  hasMore: boolean;
  itemsPerPage: number;
  nextPage?: number;
};

export function locationsEnvelope(
  query: SymbolAnchoredSemanticQuery,
  anchor: SymbolAnchor,
  kind: 'definition' | 'typeDefinition' | 'implementation',
  provider: string,
  locations: CodeSnippet[]
): LspSemanticEnvelope {
  const complete = locations.length > 0;
  const compactLocations = locations.map(compactLocation);
  const { pageItems, pagination } = paginateItems(
    compactLocations,
    query.page ?? 1,
    query.itemsPerPage ?? DEFAULT_LOCATIONS_PER_PAGE
  );
  return {
    type: query.type,
    uri: anchor.uri,
    resolvedSymbol: compactResolvedSymbol(anchor.resolvedSymbol),
    lsp: { serverAvailable: true, provider },
    payload: complete
      ? { kind, locations: pageItems }
      : {
          kind: 'empty',
          category: 'noLocations',
          reason: `${provider} returned no locations`,
        },
    ...(complete ? { pagination } : {}),
  };
}

export function referencesEnvelope(
  query: SymbolAnchoredSemanticQuery,
  anchor: SymbolAnchor,
  locations: CodeSnippet[]
): LspSemanticEnvelope {
  const refs = locations.map((location): ReferenceLocation => {
    const isDefinition =
      location.uri === anchor.uri &&
      location.range.start.line === anchor.resolvedSymbol.position.line &&
      location.range.start.character ===
        anchor.resolvedSymbol.position.character;
    return { ...location, ...(isDefinition ? { isDefinition: true } : {}) };
  });
  const byFile = query.groupByFile ? buildReferencesByFile(refs) : undefined;
  const referenceItems = byFile ?? refs.map(compactLocation);
  const { pageItems, pagination } = paginateItems(
    referenceItems,
    query.page ?? 1,
    query.itemsPerPage ?? DEFAULT_LOCATIONS_PER_PAGE
  );
  const empty =
    refs.length === 0
      ? {
          category: 'noReferences' as const,
          reason: 'referencesProvider returned no references',
        }
      : undefined;

  return {
    type: 'references',
    uri: anchor.uri,
    resolvedSymbol: compactResolvedSymbol(anchor.resolvedSymbol),
    lsp: {
      serverAvailable: true,
      provider: 'referencesProvider',
      source: 'lsp',
    },
    payload: {
      kind: 'references',
      ...(byFile ? { byFile: pageItems } : { locations: pageItems }),
      totalReferences: refs.length,
      totalFiles: new Set(refs.map(ref => ref.uri)).size,
      ...(empty ? { empty } : {}),
    },
    pagination,
  };
}

export async function hoverEnvelope(
  _query: SymbolAnchoredSemanticQuery,
  anchor: SymbolAnchor,
  hover: unknown
): Promise<LspSemanticEnvelope> {
  const normalized = normalizeHover(hover);
  const complete = Boolean(normalized.markdown || normalized.text);

  return {
    type: 'hover',
    uri: anchor.uri,
    resolvedSymbol: compactResolvedSymbol(anchor.resolvedSymbol),
    lsp: { serverAvailable: true, provider: 'hoverProvider' },
    payload: complete
      ? { kind: 'hover', ...normalized }
      : {
          kind: 'empty',
          category: 'noHover',
          reason: 'hoverProvider returned no hover content',
        },
  };
}

export async function callsEnvelope(
  query: SymbolAnchoredSemanticQuery,
  anchor: SymbolAnchor,
  client: NonNullable<Awaited<ReturnType<typeof acquirePooledClient>>>
): Promise<LspSemanticEnvelope> {
  const items = await client.prepareCallHierarchy(
    anchor.uri,
    anchor.resolvedSymbol.position,
    anchor.content
  );
  const root = items[0];
  if (!root) {
    return emptyEnvelope(query.type, anchor, 'No callable symbol found', true);
  }

  const depth = query.depth ?? 1;
  const emptyTraversal = {
    calls: [],
    truncatedByDepth: false,
    cycleCount: 0,
    failedRequestCount: 0,
  } as const;
  const incomingResult =
    query.type === 'callers' || query.type === 'callHierarchy'
      ? await gatherIncomingCallsRecursive(
          client,
          root,
          depth,
          new Set([createCallItemKey(root)]),
          query.contextLines ?? 0
        )
      : emptyTraversal;
  const outgoingResult =
    query.type === 'callees' || query.type === 'callHierarchy'
      ? await gatherOutgoingCallsRecursive(
          client,
          root,
          depth,
          new Set([createCallItemKey(root)]),
          query.contextLines ?? 0
        )
      : emptyTraversal;

  const isStdlibTarget = (call: OutgoingCall): boolean =>
    /node_modules\/typescript\/lib\/lib\.[^/]*\.d\.ts$/.test(call.to.uri);
  const stdlibCallsExcluded =
    outgoingResult.calls.filter(isStdlibTarget).length;
  const projectOutgoingCalls = outgoingResult.calls.filter(
    call => !isStdlibTarget(call)
  );

  const calls = [
    ...incomingResult.calls.map(call => ({
      direction: 'incoming' as const,
      ...call,
    })),
    ...projectOutgoingCalls.map(call => ({
      direction: 'outgoing' as const,
      ...call,
    })),
  ];
  const compactCalls = calls.map(call =>
    call.direction === 'incoming'
      ? compactIncomingCall(call, query.contextLines ?? 0)
      : compactOutgoingCall(call, query.contextLines ?? 0)
  );
  const { pageItems, pagination } = paginateItems(
    compactCalls,
    query.page ?? 1,
    query.itemsPerPage ?? DEFAULT_CALLS_PER_PAGE
  );
  const direction =
    query.type === 'callers'
      ? 'incoming'
      : query.type === 'callees'
        ? 'outgoing'
        : 'both';
  const traversalComplete =
    !incomingResult.truncatedByDepth &&
    !outgoingResult.truncatedByDepth &&
    incomingResult.failedRequestCount + outgoingResult.failedRequestCount === 0;
  return {
    type: query.type,
    uri: anchor.uri,
    resolvedSymbol: compactResolvedSymbol(anchor.resolvedSymbol),
    lsp: { serverAvailable: true, provider: 'callHierarchyProvider' },
    payload: {
      kind: query.type as 'callers' | 'callees' | 'callHierarchy',
      root: compactCallItem(root),
      direction,
      calls: pageItems,
      incomingCalls: incomingResult.calls.length,
      outgoingCalls: projectOutgoingCalls.length,
      completeness: {
        complete: traversalComplete,
        truncatedByDepth:
          incomingResult.truncatedByDepth || outgoingResult.truncatedByDepth,
        cycleCount: incomingResult.cycleCount + outgoingResult.cycleCount,
        failedRequestCount:
          incomingResult.failedRequestCount + outgoingResult.failedRequestCount,
        dynamicCallsExcluded: true,
        ...(stdlibCallsExcluded > 0 && { stdlibCallsExcluded }),
      },
      ...(calls.length === 0
        ? {
            empty: {
              category: 'noCalls' as const,
              reason: 'callHierarchyProvider returned no calls',
            },
          }
        : {}),
    },
    pagination,
  };
}

export async function typeHierarchyEnvelope(
  query: SymbolAnchoredSemanticQuery,
  anchor: SymbolAnchor,
  client: NonNullable<Awaited<ReturnType<typeof acquirePooledClient>>>
): Promise<LspSemanticEnvelope> {
  const items = await client.prepareTypeHierarchy(
    anchor.uri,
    anchor.resolvedSymbol.position,
    anchor.content
  );
  const root = items[0];
  if (!root) {
    return emptyEnvelope(
      query.type,
      anchor,
      'No type-hierarchy item found at position',
      true
    );
  }

  const direction = query.type === 'supertypes' ? 'supertypes' : 'subtypes';
  const relatives =
    direction === 'supertypes'
      ? await client.typeHierarchySupertypes(root)
      : await client.typeHierarchySubtypes(root);

  const { pageItems, pagination } = paginateItems(
    relatives,
    query.page ?? 1,
    query.itemsPerPage ?? DEFAULT_SYMBOLS_PER_PAGE
  );

  return {
    type: query.type,
    uri: anchor.uri,
    resolvedSymbol: compactResolvedSymbol(anchor.resolvedSymbol),
    lsp: { serverAvailable: true, provider: 'typeHierarchyProvider' },
    payload:
      relatives.length > 0
        ? {
            kind: 'typeHierarchy',
            direction,
            root,
            items: pageItems,
            totalItems: relatives.length,
          }
        : {
            kind: 'empty',
            category: 'noTypeHierarchy',
            reason: `typeHierarchyProvider returned no ${direction} for this symbol`,
          },
    pagination,
  };
}

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
  requestedItemsPerPage: number
): { pageItems: T[]; pagination: PaginationInfo } {
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
      itemsPerPage,
      ...(hasMore ? { nextPage: currentPage + 1 } : {}),
    },
  };
}

export function buildReferencesByFile(
  locations: readonly ReferenceLocation[]
): ReferencesByFile[] {
  const byUri = new Map<string, ReferencesByFile>();
  for (const loc of locations) {
    const lineNumber = loc.range.start.line + 1;
    const existing = byUri.get(loc.uri);
    if (existing) {
      existing.count += 1;
      existing.lines.push(lineNumber);
      if (loc.isDefinition) existing.hasDefinition = true;
      continue;
    }
    byUri.set(loc.uri, {
      uri: loc.uri,
      count: 1,
      firstLine: lineNumber,
      firstCharacter: loc.range.start.character,
      lines: [lineNumber],
      ...(loc.isDefinition ? { hasDefinition: true } : {}),
    });
  }
  return [...byUri.values()];
}

export function normalizeHover(hover: unknown): {
  markdown?: string;
  text?: string;
  range?: LSPRange;
} {
  if (!hover || typeof hover !== 'object') return {};
  const value = hover as { contents?: unknown; range?: unknown };
  const content = value.contents;
  if (typeof content === 'string') return { text: content.trim() };
  if (Array.isArray(content)) {
    return {
      markdown: content
        .map(part => stringifyHoverPart(part))
        .join('\n')
        .trim(),
    };
  }
  if (content && typeof content === 'object') {
    const part = content as { kind?: unknown; value?: unknown };
    if (typeof part.value === 'string') {
      return part.kind === 'markdown'
        ? { markdown: part.value.trim() }
        : { text: part.value.trim() };
    }
  }
  return {};
}

export function stringifyHoverPart(part: unknown): string {
  if (typeof part === 'string') return part;
  if (part && typeof part === 'object') {
    const value = (part as { value?: unknown }).value;
    if (typeof value === 'string') return value;
  }
  return String(part);
}
