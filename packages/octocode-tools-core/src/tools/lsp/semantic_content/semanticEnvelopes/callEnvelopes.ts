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
  type ConsumerWarmupStats,
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

function isTypeScriptStdlibTarget(call: OutgoingCall): boolean {
  return /node_modules\/typescript\/lib\/lib\.[^/]*\.d\.ts$/.test(call.to.uri);
}

export async function callsEnvelope(
  query: SymbolAnchoredSemanticQuery,
  anchor: SymbolAnchor,
  client: NonNullable<Awaited<ReturnType<typeof acquirePooledClient>>>,
  warmupStats?: ConsumerWarmupStats
): Promise<LspSemanticEnvelope> {
  const items = await client.prepareCallHierarchy(
    anchor.absolutePath,
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
    truncatedByBudget: false,
    visitedNodeCount: 0,
    requestCount: 0,
    excludedCallCount: 0,
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
          query.contextLines ?? 0,
          undefined,
          isTypeScriptStdlibTarget
        )
      : emptyTraversal;

  const stdlibCallsExcluded = outgoingResult.excludedCallCount ?? 0;

  const calls = [
    ...incomingResult.calls.map(call => ({
      direction: 'incoming' as const,
      ...call,
    })),
    ...outgoingResult.calls.map(call => ({
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
    query.pageSize ?? DEFAULT_CALLS_PER_PAGE,
    query,
    calls
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
    !incomingResult.truncatedByBudget &&
    !outgoingResult.truncatedByBudget &&
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
      outgoingCalls: outgoingResult.calls.length,
      ...(warmupStats ? { warmup: warmupStats } : {}),
      completeness: {
        complete: traversalComplete && !warmupStats?.possiblyTruncated,
        ...(warmupStats?.possiblyTruncated
          ? { consumerWarmupIncomplete: true as const }
          : {}),
        truncatedByDepth:
          incomingResult.truncatedByDepth || outgoingResult.truncatedByDepth,
        truncatedByBudget:
          incomingResult.truncatedByBudget || outgoingResult.truncatedByBudget,
        visitedNodeCount:
          incomingResult.visitedNodeCount + outgoingResult.visitedNodeCount,
        requestCount: incomingResult.requestCount + outgoingResult.requestCount,
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
              reason:
                stdlibCallsExcluded > 0
                  ? 'No project calls remain after excluding TypeScript standard-library targets.'
                  : 'callHierarchyProvider returned no calls',
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
    anchor.absolutePath,
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
    query.pageSize ?? DEFAULT_SYMBOLS_PER_PAGE,
    query
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
