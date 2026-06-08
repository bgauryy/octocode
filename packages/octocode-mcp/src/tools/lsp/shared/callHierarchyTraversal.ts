import type { LSPClient } from '../../../lsp/client.js';
import type {
  CallHierarchyItem,
  IncomingCall,
  OutgoingCall,
} from '../../../lsp/types.js';
import { safeReadFile } from '../../../lsp/validation.js';

export function createCallItemKey(item: CallHierarchyItem): string {
  return `${item.uri}:${item.range.start.line}:${item.name}`;
}

async function enhanceCallItem(
  item: CallHierarchyItem,
  contextLines: number
): Promise<CallHierarchyItem> {
  if (contextLines <= 0) return item;

  const content = await safeReadFile(item.uri);
  if (!content) return item;

  const lines = content.split(/\r?\n/);
  const startLine = Math.max(0, item.range.start.line - contextLines);
  const endLine = Math.min(
    lines.length - 1,
    item.range.end.line + contextLines
  );
  const snippet = lines
    .slice(startLine, endLine + 1)
    .map((line, index) => {
      const lineNumber = startLine + index + 1;
      const isTarget =
        lineNumber > item.range.start.line &&
        lineNumber <= item.range.end.line + 1;
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
      from: await enhanceCallItem(call.from, contextLines),
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
): Promise<IncomingCall[]> {
  if (remainingDepth <= 0 || !client) return [];

  try {
    const directCalls = await client.getIncomingCalls(item);
    const enhancedCalls =
      contextLines > 0
        ? await enhanceIncomingCalls(directCalls, contextLines)
        : directCalls;

    if (remainingDepth === 1) {
      return enhancedCalls;
    }

    const nestedCallGroups = await Promise.all(
      enhancedCalls.map(async call => {
        const key = createCallItemKey(call.from);
        if (visited.has(key)) return [];
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

    return [...enhancedCalls, ...nestedCallGroups.flat()];
  } catch {
    return [];
  }
}

export async function gatherOutgoingCallsRecursive(
  client: LSPClient | null,
  item: CallHierarchyItem,
  remainingDepth: number,
  visited: Set<string>,
  contextLines: number
): Promise<OutgoingCall[]> {
  if (remainingDepth <= 0 || !client) return [];

  try {
    const directCalls = await client.getOutgoingCalls(item);
    const enhancedCalls =
      contextLines > 0
        ? await enhanceOutgoingCalls(directCalls, contextLines)
        : directCalls;

    if (remainingDepth === 1) {
      return enhancedCalls;
    }

    const nestedCallGroups = await Promise.all(
      enhancedCalls.map(async call => {
        const key = createCallItemKey(call.to);
        if (visited.has(key)) return [];
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

    return [...enhancedCalls, ...nestedCallGroups.flat()];
  } catch {
    return [];
  }
}
