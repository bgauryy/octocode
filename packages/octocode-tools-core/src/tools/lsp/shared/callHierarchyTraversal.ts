import type { LSPClient } from 'octocode-lsp/client';
import type {
  CallHierarchyItem,
  IncomingCall,
  LSPRange,
  OutgoingCall,
} from 'octocode-lsp/types';
import { safeReadFile } from 'octocode-lsp/validation';

export type TraversalResult<T> = {
  calls: T[];
  truncatedByDepth: boolean;
  cycleCount: number;
  failedRequestCount: number;
};

const EMPTY_TRAVERSAL_RESULT = {
  truncatedByDepth: false,
  cycleCount: 0,
  failedRequestCount: 0,
} as const;

export function createCallItemKey(item: CallHierarchyItem): string {
  return `${item.uri}:${item.range.start.line}:${item.name}`;
}

async function enhanceCallItem(
  item: CallHierarchyItem,
  contextLines: number,
  callSiteRanges?: readonly LSPRange[]
): Promise<CallHierarchyItem> {
  if (contextLines <= 0) return item;

  const content = await safeReadFile(item.uri);
  if (!content) return item;

  const lines = content.split(/\r?\n/);

  const anchorLine = callSiteRanges?.[0]?.start.line ?? item.range.start.line;
  const startLine = Math.max(0, anchorLine - contextLines);
  const endLine = Math.min(lines.length - 1, anchorLine + contextLines);

  const snippet = lines
    .slice(startLine, endLine + 1)
    .map((line, index) => {
      const lineNumber = startLine + index + 1;
      const isTarget = lineNumber === anchorLine + 1;
      return `${isTarget ? '>' : ' '}${String(lineNumber).padStart(4, ' ')}| ${line}`;
    })
    .join('\n');

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
  contextLines: number
): Promise<TraversalResult<IncomingCall>> {
  if (remainingDepth <= 0 || !client) {
    return { calls: [], ...EMPTY_TRAVERSAL_RESULT };
  }

  try {
    const directCalls = await client.getIncomingCalls(item);
    const enhancedCalls =
      contextLines > 0
        ? await enhanceIncomingCalls(directCalls, contextLines)
        : directCalls;

    if (remainingDepth === 1) {
      return {
        calls: enhancedCalls,
        truncatedByDepth: enhancedCalls.length > 0,
        cycleCount: 0,
        failedRequestCount: 0,
      };
    }

    const nestedResults = await Promise.all(
      enhancedCalls.map(async call => {
        const key = createCallItemKey(call.from);
        if (visited.has(key)) {
          return {
            calls: [] as IncomingCall[],
            truncatedByDepth: false,
            cycleCount: 1,
            failedRequestCount: 0,
          };
        }
        visited.add(key);
        return gatherIncomingCallsRecursive(
          client,
          call.from,
          remainingDepth - 1,
          visited,
          contextLines
        );
      })
    );

    return {
      calls: [...enhancedCalls, ...nestedResults.flatMap(r => r.calls)],
      truncatedByDepth: nestedResults.some(r => r.truncatedByDepth),
      cycleCount: nestedResults.reduce((sum, r) => sum + r.cycleCount, 0),
      failedRequestCount: nestedResults.reduce(
        (sum, r) => sum + r.failedRequestCount,
        0
      ),
    };
  } catch {
    return {
      calls: [],
      truncatedByDepth: false,
      cycleCount: 0,
      failedRequestCount: 1,
    };
  }
}

export async function gatherOutgoingCallsRecursive(
  client: LSPClient | null,
  item: CallHierarchyItem,
  remainingDepth: number,
  visited: Set<string>,
  contextLines: number
): Promise<TraversalResult<OutgoingCall>> {
  if (remainingDepth <= 0 || !client) {
    return { calls: [], ...EMPTY_TRAVERSAL_RESULT };
  }

  try {
    const directCalls = await client.getOutgoingCalls(item);
    const enhancedCalls =
      contextLines > 0
        ? await enhanceOutgoingCalls(directCalls, contextLines)
        : directCalls;

    if (remainingDepth === 1) {
      return {
        calls: enhancedCalls,
        truncatedByDepth: enhancedCalls.length > 0,
        cycleCount: 0,
        failedRequestCount: 0,
      };
    }

    const nestedResults = await Promise.all(
      enhancedCalls.map(async call => {
        const key = createCallItemKey(call.to);
        if (visited.has(key)) {
          return {
            calls: [] as OutgoingCall[],
            truncatedByDepth: false,
            cycleCount: 1,
            failedRequestCount: 0,
          };
        }
        visited.add(key);
        return gatherOutgoingCallsRecursive(
          client,
          call.to,
          remainingDepth - 1,
          visited,
          contextLines
        );
      })
    );

    return {
      calls: [...enhancedCalls, ...nestedResults.flatMap(r => r.calls)],
      truncatedByDepth: nestedResults.some(r => r.truncatedByDepth),
      cycleCount: nestedResults.reduce((sum, r) => sum + r.cycleCount, 0),
      failedRequestCount: nestedResults.reduce(
        (sum, r) => sum + r.failedRequestCount,
        0
      ),
    };
  } catch {
    return {
      calls: [],
      truncatedByDepth: false,
      cycleCount: 0,
      failedRequestCount: 1,
    };
  }
}
