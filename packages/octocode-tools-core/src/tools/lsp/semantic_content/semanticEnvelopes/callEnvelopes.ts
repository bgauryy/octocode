import { acquirePooledClient } from '@octocodeai/octocode-engine/lsp/manager';
import type { OutgoingCall } from '@octocodeai/octocode-engine/lsp/types';
import {
  gatherIncomingCallsRecursive,
  gatherOutgoingCallsRecursive,
  createCallItemKey,
} from '../../shared/callHierarchyTraversal.js';
import {
  compactResolvedSymbol,
  type LspSemanticEnvelope,
  type SymbolAnchoredSemanticQuery,
} from '../../shared/semanticTypes.js';
import type { SymbolAnchor } from '../../shared/resolveSymbolAnchor.js';
import {
  compactCallItem,
  compactIncomingCall,
  compactOutgoingCall,
} from '../semanticPresentation.js';
import {
  DEFAULT_CALLS_PER_PAGE,
  DEFAULT_SYMBOLS_PER_PAGE,
  emptyEnvelope,
  paginateItems,
} from './envelopeHelpers.js';

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
