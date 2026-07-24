import type { LSPClient } from '@octocodeai/octocode-engine/lsp/client';
import type {
  CallHierarchyItem,
  IncomingCall,
  LSPRange,
  OutgoingCall,
} from '@octocodeai/octocode-engine/lsp/types';
import { safeReadLineWindow } from '@octocodeai/octocode-engine/lsp/validation';
import { splitLines } from '../../../utils/core/lines.js';

export type TraversalResult<T> = {
  calls: T[];
  truncatedByDepth: boolean;
  truncatedByBudget: boolean;
  visitedNodeCount: number;
  requestCount: number;
  cycleCount: number;
  failedRequestCount: number;
};

export type TraversalBudget = {
  maxNodes: number;
  maxRequests: number;
};

const DEFAULT_TRAVERSAL_BUDGET: TraversalBudget = {
  maxNodes: 100,
  maxRequests: 50,
};

const EMPTY_TRAVERSAL_RESULT = {
  truncatedByDepth: false,
  truncatedByBudget: false,
  visitedNodeCount: 0,
  requestCount: 0,
  cycleCount: 0,
  failedRequestCount: 0,
} as const;

export type MutableTraversalBudget = TraversalBudget & {
  visitedNodeCount: number;
  requestCount: number;
  truncatedByBudget: boolean;
};

function createMutableBudget(
  budget: Partial<TraversalBudget> | undefined
): MutableTraversalBudget {
  return {
    ...DEFAULT_TRAVERSAL_BUDGET,
    ...budget,
    visitedNodeCount: 0,
    requestCount: 0,
    truncatedByBudget: false,
  };
}

function consumeRequest(budget: MutableTraversalBudget): boolean {
  if (budget.requestCount >= budget.maxRequests) {
    budget.truncatedByBudget = true;
    return false;
  }
  budget.requestCount += 1;
  return true;
}

function consumeNode(budget: MutableTraversalBudget): boolean {
  if (budget.visitedNodeCount >= budget.maxNodes) {
    budget.truncatedByBudget = true;
    return false;
  }
  budget.visitedNodeCount += 1;
  return true;
}

function withBudget<T>(
  result: Omit<
    TraversalResult<T>,
    'visitedNodeCount' | 'requestCount' | 'truncatedByBudget'
  >,
  budget: MutableTraversalBudget
): TraversalResult<T> {
  return {
    ...result,
    truncatedByBudget: budget.truncatedByBudget,
    visitedNodeCount: budget.visitedNodeCount,
    requestCount: budget.requestCount,
  };
}

export function createCallItemKey(item: CallHierarchyItem): string {
  return `${item.uri}:${item.range.start.line}:${item.name}`;
}

async function enhanceCallItem(
  item: CallHierarchyItem,
  contextLines: number,
  callSiteRanges?: readonly LSPRange[]
): Promise<CallHierarchyItem> {
  if (contextLines <= 0) return item;

  const anchorLine = callSiteRanges?.[0]?.start.line ?? item.range.start.line;
  const snippet = await safeReadLineWindow(item.uri, anchorLine, contextLines);
  if (!snippet) return item;

  const lineCount = splitLines(snippet).length;
  const startLine = Math.max(0, anchorLine - contextLines);
  const endLine = startLine + Math.max(0, lineCount - 1);

  return {
    ...item,
    content: snippet,
    displayRange: {
      startLine: startLine + 1,
      endLine: endLine + 1,
    },
  };
}

async function enhanceIncomingCalls(
  calls: readonly IncomingCall[],
  contextLines: number
): Promise<IncomingCall[]> {
  return Promise.all(
    calls.map(async call => ({
      ...call,
      from: await enhanceCallItem(call.from, contextLines, call.fromRanges),
    }))
  );
}

async function enhanceOutgoingCalls(
  calls: readonly OutgoingCall[],
  contextLines: number
): Promise<OutgoingCall[]> {
  return Promise.all(
    calls.map(async call => ({
      ...call,
      to: await enhanceCallItem(call.to, contextLines),
    }))
  );
}

export async function gatherIncomingCallsRecursive(
  client: LSPClient | null,
  item: CallHierarchyItem,
  remainingDepth: number,
  visited: Set<string>,
  contextLines: number,
  budget: MutableTraversalBudget = createMutableBudget(undefined)
): Promise<TraversalResult<IncomingCall>> {
  if (remainingDepth <= 0 || !client) {
    return { calls: [], ...EMPTY_TRAVERSAL_RESULT };
  }
  if (!consumeRequest(budget)) {
    return withBudget(
      {
        calls: [],
        truncatedByDepth: false,
        cycleCount: 0,
        failedRequestCount: 0,
      },
      budget
    );
  }

  try {
    const directCalls = await client.getIncomingCalls(item);
    const enhancedCalls =
      contextLines > 0
        ? await enhanceIncomingCalls(directCalls, contextLines)
        : directCalls;

    if (remainingDepth === 1) {
      return withBudget(
        {
          calls: enhancedCalls,
          truncatedByDepth: enhancedCalls.length > 0,
          cycleCount: 0,
          failedRequestCount: 0,
        },
        budget
      );
    }

    const nestedResults: Array<TraversalResult<IncomingCall>> = [];
    for (const call of enhancedCalls) {
      const key = createCallItemKey(call.from);
      if (visited.has(key)) {
        nestedResults.push(
          withBudget(
            {
              calls: [] as IncomingCall[],
              truncatedByDepth: false,
              cycleCount: 1,
              failedRequestCount: 0,
            },
            budget
          )
        );
        continue;
      }
      if (!consumeNode(budget)) break;
      visited.add(key);
      nestedResults.push(
        await gatherIncomingCallsRecursive(
          client,
          call.from,
          remainingDepth - 1,
          visited,
          contextLines,
          budget
        )
      );
      if (budget.truncatedByBudget) break;
    }

    return withBudget(
      {
        calls: [...enhancedCalls, ...nestedResults.flatMap(r => r.calls)],
        truncatedByDepth: nestedResults.some(r => r.truncatedByDepth),
        cycleCount: nestedResults.reduce((sum, r) => sum + r.cycleCount, 0),
        failedRequestCount: nestedResults.reduce(
          (sum, r) => sum + r.failedRequestCount,
          0
        ),
      },
      budget
    );
  } catch {
    return withBudget(
      {
        calls: [],
        truncatedByDepth: false,
        cycleCount: 0,
        failedRequestCount: 1,
      },
      budget
    );
  }
}

export async function gatherOutgoingCallsRecursive(
  client: LSPClient | null,
  item: CallHierarchyItem,
  remainingDepth: number,
  visited: Set<string>,
  contextLines: number,
  budget: MutableTraversalBudget = createMutableBudget(undefined)
): Promise<TraversalResult<OutgoingCall>> {
  if (remainingDepth <= 0 || !client) {
    return { calls: [], ...EMPTY_TRAVERSAL_RESULT };
  }
  if (!consumeRequest(budget)) {
    return withBudget(
      {
        calls: [],
        truncatedByDepth: false,
        cycleCount: 0,
        failedRequestCount: 0,
      },
      budget
    );
  }

  try {
    const directCalls = await client.getOutgoingCalls(item);
    const enhancedCalls =
      contextLines > 0
        ? await enhanceOutgoingCalls(directCalls, contextLines)
        : directCalls;

    if (remainingDepth === 1) {
      return withBudget(
        {
          calls: enhancedCalls,
          truncatedByDepth: enhancedCalls.length > 0,
          cycleCount: 0,
          failedRequestCount: 0,
        },
        budget
      );
    }

    const nestedResults: Array<TraversalResult<OutgoingCall>> = [];
    for (const call of enhancedCalls) {
      const key = createCallItemKey(call.to);
      if (visited.has(key)) {
        nestedResults.push(
          withBudget(
            {
              calls: [] as OutgoingCall[],
              truncatedByDepth: false,
              cycleCount: 1,
              failedRequestCount: 0,
            },
            budget
          )
        );
        continue;
      }
      if (!consumeNode(budget)) break;
      visited.add(key);
      nestedResults.push(
        await gatherOutgoingCallsRecursive(
          client,
          call.to,
          remainingDepth - 1,
          visited,
          contextLines,
          budget
        )
      );
      if (budget.truncatedByBudget) break;
    }

    return withBudget(
      {
        calls: [...enhancedCalls, ...nestedResults.flatMap(r => r.calls)],
        truncatedByDepth: nestedResults.some(r => r.truncatedByDepth),
        cycleCount: nestedResults.reduce((sum, r) => sum + r.cycleCount, 0),
        failedRequestCount: nestedResults.reduce(
          (sum, r) => sum + r.failedRequestCount,
          0
        ),
      },
      budget
    );
  } catch {
    return withBudget(
      {
        calls: [],
        truncatedByDepth: false,
        cycleCount: 0,
        failedRequestCount: 1,
      },
      budget
    );
  }
}
