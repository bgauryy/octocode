/**
 * LSP call hierarchy implementation - core LSP protocol calls
 */

import { readFile } from 'node:fs/promises';
import { getHints } from '../../hints/index.js';
import { acquirePooledClient } from '../../lsp/manager.js';
import type { LSPClient } from '../../lsp/client.js';
import { LSP_ERROR_CODES } from '../../lsp/lspErrorCodes.js';
import type {
  CallHierarchyResult,
  CallHierarchyItem,
  IncomingCall,
  OutgoingCall,
  ExactPosition,
  CodeSnippet,
} from '../../lsp/types.js';
import type { LSPCallHierarchyQuery } from '@octocodeai/octocode-core';
import {
  createCallItemKey,
  enhanceCallHierarchyItem,
  enhanceIncomingCalls,
  enhanceOutgoingCalls,
  paginateResults,
} from './callHierarchyHelpers.js';
import { TOOL_NAME } from './constants.js';

/**
 * Use LSP client for semantic call hierarchy
 */
export async function callHierarchyWithLSP(
  filePath: string,
  workspaceRoot: string,
  position: ExactPosition,
  query: LSPCallHierarchyQuery,
  content: string
): Promise<CallHierarchyResult | null> {
  // Pooled client: the pool owns its lifecycle, so we MUST NOT stop() it
  // here. Idle eviction tears it down later (see lsp/lspClientPool.ts).
  const client = await acquirePooledClient(workspaceRoot, filePath);
  if (!client) return null;

  try {
    let items = await client.prepareCallHierarchy(filePath, position);
    let effectiveContent = content;

    // Auto-follow: if no callable symbol at position (e.g. import line),
    // try gotoDefinition to follow to the actual declaration and retry
    if (!items || items.length === 0) {
      const followed = await tryFollowToDefinition(client, filePath, position);
      if (followed) {
        items = followed.items;
        if (followed.content) effectiveContent = followed.content;
      }
    }

    if (!items || items.length === 0) {
      return {
        status: 'empty',
        error: 'No callable symbol found at position',
        errorType: 'symbol_not_found',
        errorCode: LSP_ERROR_CODES.SYMBOL_NOT_FOUND,
        direction: query.direction,
        depth: query.depth ?? 1,
        hints: [
          ...getHints(TOOL_NAME, 'empty'),
          'Language server could not identify a callable symbol',
          'Ensure the position is on a function/method name',
          'Try adjusting lineHint to the exact function declaration line',
          'If pointing at an import, run lspGotoDefinition first and use the definition lineHint',
        ],
      };
    }

    const targetItem = items[0]!;

    const enhancedTargetItem = await enhanceCallHierarchyItem(
      targetItem,
      effectiveContent,
      query.contextLines ?? 2
    );

    const depth = query.depth ?? 1;
    const visited = new Set<string>();
    visited.add(createCallItemKey(targetItem));

    if (query.direction === 'incoming') {
      const contextLines = query.contextLines ?? 2;
      const callsPerPage = query.callsPerPage ?? 15;
      const page = query.page ?? 1;

      const allIncomingCalls = await gatherIncomingCallsRecursive(
        client,
        targetItem,
        depth,
        visited,
        0 // Gather without content enhancement
      );

      if (allIncomingCalls.length === 0) {
        return stripCallHierarchyInternalFields({
          status: 'empty',
          item: enhancedTargetItem,
          direction: 'incoming',
          depth,
          incomingCalls: [],
          hints: [
            ...getHints(TOOL_NAME, 'empty', { direction: 'incoming' }),
            `No callers found for '${query.symbolName}' via Language Server`,
            'The function may not be called directly in the workspace',
            'Check if it is called via alias or dynamic invocation',
            'Try lspFindReferences for broader usage search',
          ],
        });
      }

      const totalPages = Math.ceil(allIncomingCalls.length / callsPerPage);
      if (page > totalPages) {
        return stripCallHierarchyInternalFields({
          status: 'empty',
          item: enhancedTargetItem,
          direction: 'incoming',
          depth,
          incomingCalls: [],
          pagination: {
            currentPage: page,
            totalPages,
            totalResults: allIncomingCalls.length,
            hasMore: false,
            resultsPerPage: callsPerPage,
          },
          hints: [
            ...getHints(TOOL_NAME, 'empty', {
              direction: 'incoming',
            }),
            `Requested page ${page} is outside available range (1-${totalPages}).`,
            `Use page=${totalPages} for the last available page.`,
          ],
        });
      }

      const { paginatedItems, pagination } = paginateResults(
        allIncomingCalls,
        callsPerPage,
        page
      );

      const enhancedItems =
        contextLines > 0
          ? await enhanceIncomingCalls(paginatedItems, contextLines)
          : paginatedItems;

      return stripCallHierarchyInternalFields({
        status: 'hasResults',
        item: enhancedTargetItem,
        direction: 'incoming',
        depth,
        incomingCalls: enhancedItems,
        pagination,
        hints: [
          ...getHints(TOOL_NAME, 'hasResults', {
            direction: 'incoming',
            callCount: allIncomingCalls.length,
            depth,
            hasMorePages: pagination ? pagination.totalPages > 1 : false,
            currentPage: pagination?.currentPage,
            totalPages: pagination?.totalPages,
          }),
          `Found ${allIncomingCalls.length} caller(s) via Language Server (depth ${depth})`,
          'Each incomingCall.from = a function that calls this symbol; fromRanges = exact call sites',
          'Use lspGotoDefinition to navigate to each caller',
        ],
      });
    } else {
      const contextLines = query.contextLines ?? 2;
      const callsPerPage = query.callsPerPage ?? 15;
      const page = query.page ?? 1;

      const allOutgoingCalls = await gatherOutgoingCallsRecursive(
        client,
        targetItem,
        depth,
        visited,
        0 // Gather without content enhancement
      );

      if (allOutgoingCalls.length === 0) {
        return stripCallHierarchyInternalFields({
          status: 'empty',
          item: enhancedTargetItem,
          direction: 'outgoing',
          depth,
          outgoingCalls: [],
          hints: [
            ...getHints(TOOL_NAME, 'empty', { direction: 'outgoing' }),
            `No callees found in '${query.symbolName}' via Language Server`,
            'The function may only contain primitive operations',
            'Check if calls use dynamic invocation patterns',
          ],
        });
      }

      const totalPages = Math.ceil(allOutgoingCalls.length / callsPerPage);
      if (page > totalPages) {
        return stripCallHierarchyInternalFields({
          status: 'empty',
          item: enhancedTargetItem,
          direction: 'outgoing',
          depth,
          outgoingCalls: [],
          pagination: {
            currentPage: page,
            totalPages,
            totalResults: allOutgoingCalls.length,
            hasMore: false,
            resultsPerPage: callsPerPage,
          },
          hints: [
            ...getHints(TOOL_NAME, 'empty', {
              direction: 'outgoing',
            }),
            `Requested page ${page} is outside available range (1-${totalPages}).`,
            `Use page=${totalPages} for the last available page.`,
          ],
        });
      }

      const { paginatedItems, pagination } = paginateResults(
        allOutgoingCalls,
        callsPerPage,
        page
      );

      const enhancedItems =
        contextLines > 0
          ? await enhanceOutgoingCalls(paginatedItems, contextLines)
          : paginatedItems;

      return stripCallHierarchyInternalFields({
        status: 'hasResults',
        item: enhancedTargetItem,
        direction: 'outgoing',
        depth,
        outgoingCalls: enhancedItems,
        pagination,
        hints: [
          ...getHints(TOOL_NAME, 'hasResults', {
            direction: 'outgoing',
            callCount: allOutgoingCalls.length,
            depth,
            hasMorePages: pagination ? pagination.totalPages > 1 : false,
            currentPage: pagination?.currentPage,
            totalPages: pagination?.totalPages,
          }),
          `Found ${allOutgoingCalls.length} callee(s) via Language Server (depth ${depth})`,
          'Each outgoingCall.to = a function called by this symbol; fromRanges = exact call sites',
          'Use lspGotoDefinition to navigate to each callee',
        ],
      });
    }
  } catch {
    // Preserve existing fallback contract: caller falls back to pattern matching
    // when LSP path fails for any reason. The pool owns lifecycle, so no
    // stop() call here — idle eviction handles teardown.
    return null;
  }
}

/**
 * Recursively gather incoming calls with cycle detection.
 * Returns a flattened list of all callers up to the specified depth.
 */
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

    const allCalls: IncomingCall[] = [...enhancedCalls];
    const nestedCallGroups = await Promise.all(
      enhancedCalls.map(async call => {
        const key = createCallItemKey(call.from);
        if (visited.has(key)) return []; // Skip cycles
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

    for (const nestedCalls of nestedCallGroups) {
      allCalls.push(...nestedCalls);
    }

    return allCalls;
  } catch {
    return [];
  }
}

/**
 * Recursively gather outgoing calls with cycle detection.
 * Returns a flattened list of all callees up to the specified depth.
 */
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

    const allCalls: OutgoingCall[] = [...enhancedCalls];
    const nestedCallGroups = await Promise.all(
      enhancedCalls.map(async call => {
        const key = createCallItemKey(call.to);
        if (visited.has(key)) return []; // Skip cycles
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

    for (const nestedCalls of nestedCallGroups) {
      allCalls.push(...nestedCalls);
    }

    return allCalls;
  } catch {
    return [];
  }
}

/**
 * When prepareCallHierarchy returns empty (e.g. position is on an import),
 * try gotoDefinition to follow to the actual declaration and retry.
 */
async function tryFollowToDefinition(
  client: LSPClient,
  filePath: string,
  position: ExactPosition
): Promise<{
  items: CallHierarchyItem[];
  content?: string;
} | null> {
  try {
    const definitions: CodeSnippet[] = await client.gotoDefinition(
      filePath,
      position
    );
    if (!definitions || definitions.length === 0) return null;

    const def = definitions[0]!;
    if (!def.uri || !def.range) return null;

    const defPosition: ExactPosition = {
      line: def.range.start.line,
      character: def.range.start.character,
    };

    const defItems = await client.prepareCallHierarchy(def.uri, defPosition);
    if (!defItems || defItems.length === 0) return null;

    let content: string | undefined;
    if (def.uri !== filePath) {
      try {
        content = await readFile(def.uri, 'utf-8');
      } catch {
        // Definition file unreadable; omit extra snippet text, items are still valid.
      }
    }

    return { items: defItems, content };
  } catch {
    // gotoDefinition or prepareCallHierarchy failed after import follow; no definition hop available.
    return null;
  }
}

/**
 * Strip internal LSP fields from call hierarchy results before returning.
 * Removes selectionRange and displayRange which are not useful for LLM consumers.
 */
function stripCallHierarchyInternalFields(
  result: CallHierarchyResult
): CallHierarchyResult {
  const stripItem = (item: CallHierarchyItem): CallHierarchyItem => {
    const { selectionRange: _sel, displayRange: _disp, ...rest } = item;
    return rest;
  };

  return {
    ...result,
    ...(result.item && { item: stripItem(result.item) }),
    ...(result.incomingCalls && {
      incomingCalls: result.incomingCalls.map(call => ({
        ...call,
        from: stripItem(call.from),
      })),
    }),
    ...(result.outgoingCalls && {
      outgoingCalls: result.outgoingCalls.map(call => ({
        ...call,
        to: stripItem(call.to),
      })),
    }),
  };
}
