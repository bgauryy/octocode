import { safeReadFile } from '../../lsp/validation.js';
import type {
  CallHierarchyItem,
  IncomingCall,
  OutgoingCall,
  LSPPaginationInfo,
} from '../../lsp/types.js';

export function createCallItemKey(item: CallHierarchyItem): string {
  return `${item.uri}:${item.range.start.line}:${item.name}`;
}

export async function enhanceCallHierarchyItem(
  item: CallHierarchyItem,
  content: string,
  contextLines: number
): Promise<CallHierarchyItem> {
  const lines = content.split(/\r?\n/);
  const startLine = Math.max(0, item.range.start.line - contextLines);
  const endLine = Math.min(
    lines.length - 1,
    item.range.end.line + contextLines
  );

  const snippetLines = lines.slice(startLine, endLine + 1);
  const numberedContent = snippetLines
    .map((line, i) => {
      const lineNum = startLine + i + 1;
      const isTarget =
        lineNum > item.range.start.line && lineNum <= item.range.end.line + 1;
      const marker = isTarget ? '>' : ' ';
      return `${marker}${String(lineNum).padStart(4, ' ')}| ${line}`;
    })
    .join('\n');

  return {
    ...item,
    content: numberedContent,
    displayRange: {
      startLine: startLine + 1,
      endLine: endLine + 1,
    },
  };
}

export async function enhanceIncomingCalls(
  calls: IncomingCall[],
  contextLines: number
): Promise<IncomingCall[]> {
  return enhanceCalls(
    calls,
    contextLines,
    call => call.from,
    (call, enhancedItem) => ({
      ...call,
      from: enhancedItem,
    })
  );
}

export async function enhanceOutgoingCalls(
  calls: OutgoingCall[],
  contextLines: number
): Promise<OutgoingCall[]> {
  return enhanceCalls(
    calls,
    contextLines,
    call => call.to,
    (call, enhancedItem) => ({
      ...call,
      to: enhancedItem,
    })
  );
}

async function enhanceCalls<T>(
  calls: T[],
  contextLines: number,
  getItem: (call: T) => CallHierarchyItem,
  applyEnhancedItem: (call: T, enhancedItem: CallHierarchyItem) => T
): Promise<T[]> {
  return Promise.all(
    calls.map(async call => {
      try {
        const item = getItem(call);
        const fileContent = await safeReadFile(item.uri);
        if (!fileContent) {
          return call;
        }
        const enhancedItem = await enhanceCallHierarchyItem(
          item,
          fileContent,
          contextLines
        );
        return applyEnhancedItem(call, enhancedItem);
      } catch {
        return call;
      }
    })
  );
}

export function paginateResults<T>(
  items: T[],
  perPage: number,
  page: number
): { paginatedItems: T[]; pagination: LSPPaginationInfo } {
  const totalResults = items.length;
  const totalPages = Math.ceil(totalResults / perPage);
  const startIndex = (page - 1) * perPage;
  const paginatedItems = items.slice(startIndex, startIndex + perPage);

  return {
    paginatedItems,
    pagination: {
      currentPage: page,
      totalPages,
      totalResults,
      hasMore: page < totalPages,
      resultsPerPage: perPage,
    },
  };
}
