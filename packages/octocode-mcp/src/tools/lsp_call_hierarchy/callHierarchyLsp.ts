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
import type { z } from 'zod';
import type { LSPCallHierarchyQuerySchema } from '@octocodeai/octocode-core/schemas';

type LSPCallHierarchyQuery = z.infer<typeof LSPCallHierarchyQuerySchema>;
import type { WithOptionalMeta } from '../../types/execution.js';
import {
  createCallItemKey,
  enhanceCallHierarchyItem,
  enhanceIncomingCalls,
  enhanceOutgoingCalls,
  paginateResults,
} from './callHierarchyHelpers.js';
import { TOOL_NAME } from './constants.js';

function buildCapabilityErrorResult(
  query: WithOptionalMeta<LSPCallHierarchyQuery>
): CallHierarchyResult {
  return {
    status: 'error',
    error: 'Language server does not support call hierarchy',
    errorType: 'not_a_function',
    errorCode: LSP_ERROR_CODES.LSP_CAPABILITY_UNSUPPORTED,
    direction: query.direction,
    depth: query.depth ?? 1,
    hints: [
      ...getHints(TOOL_NAME, 'error'),
      'The active language server does not advertise callHierarchyProvider.',
      'Try lspFindReferences for broader usage analysis.',
    ],
  };
}

function buildNoSymbolResult(
  query: WithOptionalMeta<LSPCallHierarchyQuery>
): CallHierarchyResult {
  return {
    status: 'empty',
    error: 'No callable symbol found at position',
    errorType: 'symbol_not_found',
    errorCode: LSP_ERROR_CODES.SYMBOL_NOT_FOUND,
    direction: query.direction,
    depth: query.depth ?? 1,
    hints: [
      ...getHints(TOOL_NAME, 'empty'),
      'lineHint must point to the function name line — run localSearchCode to get the exact line, then retry.',
      'If pointing at an import, run lspGotoDefinition first to resolve to the definition line.',
    ],
  };
}

async function resolveIncomingCalls(
  client: LSPClient,
  targetItem: CallHierarchyItem,
  enhancedTargetItem: CallHierarchyItem,
  query: WithOptionalMeta<LSPCallHierarchyQuery>,
  depth: number,
  visited: Set<string>
): Promise<CallHierarchyResult> {
  const contextLines = query.contextLines ?? 2;
  const callsPerPage = query.callsPerPage ?? 15;
  const page = query.page ?? 1;

  const allIncomingCalls = await gatherIncomingCallsRecursive(
    client,
    targetItem,
    depth,
    visited,
    0
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
        `No static callers found for '${query.symbolName}' — use lspFindReferences for broader usage search.`,
        'Dynamic callers (await import(...), require(), event handlers) are invisible to LSP call hierarchy — use localSearchCode to find them textually.',
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
        ...getHints(TOOL_NAME, 'empty', { direction: 'incoming' }),
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
    item: enhancedTargetItem,
    direction: 'incoming',
    depth,
    incomingCalls: enhancedItems,
    pagination,
    hints: [
      ...(enhancedItems.length > 0
        ? [
            `Found ${enhancedItems.length} caller${enhancedItems.length === 1 ? '' : 's'} across ${new Set(enhancedItems.map(c => c.from.uri)).size} file${new Set(enhancedItems.map(c => c.from.uri)).size === 1 ? '' : 's'} — use depth=1 to limit tracing, or narrow with a different symbol.`,
          ]
        : []),
      ...(pagination && pagination.totalPages > 1
        ? [
            `Showing page ${pagination.currentPage} of ${pagination.totalPages}. Use page=${(pagination.currentPage ?? 1) + 1} for more.`,
          ]
        : []),
    ],
  });
}

async function resolveOutgoingCalls(
  client: LSPClient,
  targetItem: CallHierarchyItem,
  enhancedTargetItem: CallHierarchyItem,
  query: WithOptionalMeta<LSPCallHierarchyQuery>,
  depth: number,
  visited: Set<string>
): Promise<CallHierarchyResult> {
  const contextLines = query.contextLines ?? 2;
  const callsPerPage = query.callsPerPage ?? 15;
  const page = query.page ?? 1;

  const allOutgoingCalls = await gatherOutgoingCallsRecursive(
    client,
    targetItem,
    depth,
    visited,
    0
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
        `No outgoing calls found in '${query.symbolName}' — dynamic calls (await import(...), require()) are invisible to LSP.`,
        'Use localSearchCode to find dynamic call sites, or retry at the definition returned by lspGotoDefinition.',
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
        ...getHints(TOOL_NAME, 'empty', { direction: 'outgoing' }),
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
    item: enhancedTargetItem,
    direction: 'outgoing',
    depth,
    outgoingCalls: enhancedItems,
    pagination,
    hints: [
      ...(enhancedItems.length > 0
        ? [
            `Found ${enhancedItems.length} callee${enhancedItems.length === 1 ? '' : 's'} across ${new Set(enhancedItems.map(c => c.to.uri)).size} file${new Set(enhancedItems.map(c => c.to.uri)).size === 1 ? '' : 's'} — use depth=1 to limit tracing, or narrow with a different symbol.`,
          ]
        : []),
      ...(pagination && pagination.totalPages > 1
        ? [
            `Showing page ${pagination.currentPage} of ${pagination.totalPages}. Use page=${(pagination.currentPage ?? 1) + 1} for more.`,
          ]
        : []),
    ],
  });
}

export async function callHierarchyWithLSP(
  filePath: string,
  workspaceRoot: string,
  position: ExactPosition,
  query: WithOptionalMeta<LSPCallHierarchyQuery>,
  content: string
): Promise<CallHierarchyResult | null> {
  const client = await acquirePooledClient(workspaceRoot, filePath);
  if (!client) return null;

  if (client.hasCapability && !client.hasCapability('callHierarchyProvider')) {
    return buildCapabilityErrorResult(query);
  }

  try {
    let items = await client.prepareCallHierarchy(filePath, position);
    let effectiveContent = content;

    if (!items || items.length === 0) {
      const followed = await tryFollowToDefinition(client, filePath, position);
      if (followed) {
        items = followed.items;
        if (followed.content) effectiveContent = followed.content;
      }
    }

    if (!items || items.length === 0) {
      return buildNoSymbolResult(query);
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

    return query.direction === 'incoming'
      ? await resolveIncomingCalls(
          client,
          targetItem,
          enhancedTargetItem,
          query,
          depth,
          visited
        )
      : await resolveOutgoingCalls(
          client,
          targetItem,
          enhancedTargetItem,
          query,
          depth,
          visited
        );
  } catch {
    return null;
  }
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

    const allCalls: IncomingCall[] = [...enhancedCalls];
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

    for (const nestedCalls of nestedCallGroups) {
      allCalls.push(...nestedCalls);
    }

    return allCalls;
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

    const allCalls: OutgoingCall[] = [...enhancedCalls];
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

    for (const nestedCalls of nestedCallGroups) {
      allCalls.push(...nestedCalls);
    }

    return allCalls;
  } catch {
    return [];
  }
}

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
        void 0;
      }
    }

    return { items: defItems, content };
  } catch {
    return null;
  }
}

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
