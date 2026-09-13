import { acquirePooledClient } from '@octocodeai/octocode-engine/lsp/manager';
import type {
  CallHierarchyItem,
  IncomingCall,
  OutgoingCall,
} from '@octocodeai/octocode-engine/lsp/types';
import {
  gatherIncomingCallsRecursive,
  gatherOutgoingCallsRecursive,
  createCallItemKey,
  createMutableBudget,
  type MutableTraversalBudget,
  type TraversalResult,
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

function callIdentity(call: IncomingCall | OutgoingCall): string {
  const target = 'from' in call ? call.from : call.to;
  const rangeKey = (range: CallHierarchyItem['range']) => [
    range.start.line,
    range.start.character,
    range.end.line,
    range.end.character,
  ];
  return JSON.stringify([
    target.uri,
    target.name,
    rangeKey(target.range),
    call.fromRanges.map(rangeKey).sort(),
  ]);
}

function typeIdentity(item: unknown): string {
  const value = item as Partial<CallHierarchyItem> | null;
  const range = value?.range;
  return value?.uri && range?.start && range.end
    ? JSON.stringify([
        value.uri,
        value.name,
        range.start.line,
        range.start.character,
        range.end.line,
        range.end.character,
      ])
    : (JSON.stringify(item) ?? String(item));
}

async function gatherPreparedRoots<T extends IncomingCall | OutgoingCall>(
  roots: CallHierarchyItem[],
  gather: (
    root: CallHierarchyItem,
    visited: Set<string>,
    budget: MutableTraversalBudget
  ) => Promise<TraversalResult<T>>
): Promise<TraversalResult<T>> {
  const budget = createMutableBudget(undefined);
  const visited = new Set(roots.map(createCallItemKey));
  const results: Array<TraversalResult<T>> = [];
  for (const root of roots) {
    results.push(await gather(root, visited, budget));
    if (budget.truncatedByBudget) break;
  }
  const calls = new Map<string, T>();
  for (const result of results) {
    for (const call of result.calls) calls.set(callIdentity(call), call);
  }
  return {
    calls: [...calls.values()],
    truncatedByDepth: results.some(result => result.truncatedByDepth),
    truncatedByBudget: budget.truncatedByBudget,
    visitedNodeCount: budget.visitedNodeCount,
    requestCount: budget.requestCount,
    cycleCount: results.reduce((total, result) => total + result.cycleCount, 0),
    failedRequestCount: results.reduce(
      (total, result) => total + result.failedRequestCount,
      0
    ),
    excludedCallCount: results.reduce(
      (total, result) => total + (result.excludedCallCount ?? 0),
      0
    ),
  };
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
    return emptyEnvelope(
      query.operation,
      anchor,
      'No callable symbol found',
      'noCalls',
      true
    );
  }

  // Every prepared root can expose distinct relationships. Bound root fanout
  // with the existing request budget and report omitted roots explicitly.
  const roots = items.slice(0, createMutableBudget(undefined).maxRequests);
  const rootsTruncated = roots.length < items.length;

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
    query.operation === 'callers' || query.operation === 'callHierarchy'
      ? await gatherPreparedRoots(roots, (prepared, visited, budget) =>
          gatherIncomingCallsRecursive(
            client,
            prepared,
            depth,
            visited,
            query.contextLines ?? 0,
            budget
          )
        )
      : emptyTraversal;
  const outgoingResult =
    query.operation === 'callees' || query.operation === 'callHierarchy'
      ? await gatherPreparedRoots(roots, (prepared, visited, budget) =>
          gatherOutgoingCallsRecursive(
            client,
            prepared,
            depth,
            visited,
            query.contextLines ?? 0,
            budget,
            isTypeScriptStdlibTarget
          )
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
    query.operation === 'callers'
      ? 'incoming'
      : query.operation === 'callees'
        ? 'outgoing'
        : 'both';
  const traversalComplete =
    !rootsTruncated &&
    !incomingResult.truncatedByDepth &&
    !outgoingResult.truncatedByDepth &&
    !incomingResult.truncatedByBudget &&
    !outgoingResult.truncatedByBudget &&
    incomingResult.failedRequestCount + outgoingResult.failedRequestCount === 0;
  return {
    type: query.operation,
    uri: anchor.uri,
    resolvedSymbol: compactResolvedSymbol(anchor.resolvedSymbol),
    lsp: { serverAvailable: true, provider: 'callHierarchyProvider' },
    ...(incomingResult.failedRequestCount + outgoingResult.failedRequestCount >
    0
      ? { incompleteResults: true }
      : {}),
    payload: {
      kind: query.operation as 'callers' | 'callees' | 'callHierarchy',
      ...(items.length === 1
        ? { root: compactCallItem(root) }
        : { roots: roots.map(compactCallItem) }),
      direction,
      calls: pageItems,
      incomingCalls: incomingResult.calls.length,
      outgoingCalls: outgoingResult.calls.length,
      ...(warmupStats ? { warmup: warmupStats } : {}),
      completeness: {
        complete: traversalComplete && !warmupStats?.possiblyTruncated,
        ...(items.length > 1 ? { preparedRootCount: items.length } : {}),
        ...(warmupStats?.possiblyTruncated
          ? { consumerWarmupIncomplete: true as const }
          : {}),
        truncatedByDepth:
          incomingResult.truncatedByDepth || outgoingResult.truncatedByDepth,
        truncatedByBudget:
          rootsTruncated ||
          incomingResult.truncatedByBudget ||
          outgoingResult.truncatedByBudget,
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
      query.operation,
      anchor,
      'No type-hierarchy item found at position',
      'noTypeHierarchy',
      true
    );
  }

  const direction =
    query.operation === 'supertypes' ? 'supertypes' : 'subtypes';
  const roots = items.slice(0, createMutableBudget(undefined).maxRequests);
  const truncatedByBudget = roots.length < items.length;
  let failedRequestCount = 0;
  const uniqueRelatives = new Map<string, unknown>();
  for (const prepared of roots) {
    try {
      const related =
        direction === 'supertypes'
          ? await client.typeHierarchySupertypes(prepared)
          : await client.typeHierarchySubtypes(prepared);
      for (const relative of related) {
        uniqueRelatives.set(typeIdentity(relative), relative);
      }
    } catch {
      failedRequestCount++;
    }
  }
  const relatives = [...uniqueRelatives.values()];
  const complete = !truncatedByBudget && failedRequestCount === 0;

  const { pageItems, pagination } = paginateItems(
    relatives,
    query.page ?? 1,
    query.pageSize ?? DEFAULT_SYMBOLS_PER_PAGE,
    query
  );

  return {
    type: query.operation,
    uri: anchor.uri,
    resolvedSymbol: compactResolvedSymbol(anchor.resolvedSymbol),
    lsp: { serverAvailable: true, provider: 'typeHierarchyProvider' },
    ...(!complete ? { incompleteResults: true } : {}),
    ...(truncatedByBudget
      ? { terminalLimit: true, partialReasons: ['budget' as const] }
      : {}),
    payload:
      relatives.length > 0 || items.length > 1 || !complete
        ? {
            kind: 'typeHierarchy',
            direction,
            ...(items.length === 1 ? { root } : { roots }),
            items: pageItems,
            totalItems: relatives.length,
            completeness: {
              complete,
              preparedRootCount: items.length,
              requestCount: roots.length,
              failedRequestCount,
              truncatedByBudget,
            },
            ...(relatives.length === 0
              ? {
                  empty: {
                    category: complete
                      ? ('noTypeHierarchy' as const)
                      : ('possiblyIncomplete' as const),
                    reason: complete
                      ? `typeHierarchyProvider returned no ${direction} for these roots`
                      : 'Some prepared roots could not be queried; missing types are not proof of absence.',
                  },
                }
              : {}),
          }
        : {
            kind: 'empty',
            category: 'noTypeHierarchy',
            reason: `typeHierarchyProvider returned no ${direction} for this symbol`,
          },
    pagination,
  };
}
