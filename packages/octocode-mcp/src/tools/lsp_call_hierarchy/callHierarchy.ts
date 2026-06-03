/**
 * LSP Call Hierarchy tool - traces function call relationships
 * Uses Language Server Protocol for semantic call hierarchy discovery
 * Falls back to pattern matching when LSP is unavailable
 */

import { readFile } from 'fs/promises';
import { getHints } from '../../hints/index.js';
import {
  validateToolPath,
  createErrorResult,
} from '../../utils/file/toolHelpers.js';
import { SymbolResolver, SymbolResolutionError } from '../../lsp/resolver.js';
import {
  isLanguageServerAvailable,
  LSP_UNAVAILABLE_HINT,
} from '../../lsp/manager.js';
import type {
  CallHierarchyResult,
  IncomingCall,
  OutgoingCall,
} from '../../lsp/types.js';
import type { z } from 'zod';
import type { LSPCallHierarchyQuerySchema } from '@octocodeai/octocode-core/schemas';

type UpstreamLSPCallHierarchyQuery = z.infer<
  typeof LSPCallHierarchyQuerySchema
>;
import type { WithVerbosity } from '../../scheme/localSchemaOverlay.js';
import type { WithOptionalMeta } from '../../types/execution.js';
import {
  isConcise,
  isCompact,
  compactTrimHints,
  makeAdvisoryPredicate,
} from '../../scheme/verbosity.js';

/** Advisory hints lspCallHierarchy emits; stripped under compact.
 * Substring-OR, case-insensitive. */
const isAdvisoryCallHierarchyHint = makeAdvisoryPredicate([
  'prefer depth=1',
  'risks timeouts',
  'hot function',
  'fallback',
]);

type LSPCallHierarchyQuery = WithVerbosity<
  WithOptionalMeta<UpstreamLSPCallHierarchyQuery>
> & {
  orderHint?: number;
};
import { ToolErrors } from '../../errors/errorFactories.js';
import { callHierarchyWithLSP } from './callHierarchyLsp.js';
import { TOOL_NAME } from './constants.js';
import { resolveWorkspaceRootForFile } from '../../lsp/workspaceRoot.js';
import { LSP_ERROR_CODES } from '../../lsp/lspErrorCodes.js';
import {
  attachRawResponseChars,
  countSerializedChars,
  getRawResponseChars,
} from '../../utils/response/charSavings.js';

/**
 * Process a single call hierarchy query.
 *
 * Wraps the internal core logic with the verbosity transformer so that
 * `verbosity:"concise"` returns graph edges only (no per-node content).
 */
export async function processCallHierarchy(
  query: LSPCallHierarchyQuery
): Promise<CallHierarchyResult> {
  // Surface page-size knob is the cross-tool `itemsPerPage`; the internal
  // pipeline threads `callsPerPage`. Bridge once here so downstream logic is
  // unchanged.
  const bridge = query as { itemsPerPage?: number; callsPerPage?: number };
  if (
    bridge.callsPerPage === undefined &&
    typeof bridge.itemsPerPage === 'number'
  ) {
    bridge.callsPerPage = bridge.itemsPerPage;
  }
  const result = await processCallHierarchyInternal(query);
  const rawChars = getRawResponseChars(result) ?? countSerializedChars(result);
  return attachRawResponseChars(
    applyCallHierarchyVerbosity(result, query),
    rawChars
  );
}

async function processCallHierarchyInternal(
  query: LSPCallHierarchyQuery
): Promise<CallHierarchyResult> {
  try {
    const pathValidation = validateToolPath(
      { path: query.uri, ...query },
      TOOL_NAME
    );
    if (!pathValidation.isValid) {
      return pathValidation.errorResult as CallHierarchyResult;
    }

    const absolutePath = pathValidation.sanitizedPath!;
    const uri = query.uri!;
    const symbolName = query.symbolName!;
    const lineHint = query.lineHint!;

    let content: string;
    try {
      content = await readFile(absolutePath, 'utf-8');
    } catch (error) {
      const toolError = ToolErrors.fileAccessFailed(
        uri,
        error instanceof Error ? error : undefined
      );
      return createErrorResult(toolError, query, {
        toolName: TOOL_NAME,
        extra: { resolvedPath: absolutePath },
      }) as CallHierarchyResult;
    }

    const resolver = new SymbolResolver({ lineSearchRadius: 5 });
    let resolvedSymbol;
    try {
      resolvedSymbol = resolver.resolvePositionFromContent(content, {
        symbolName,
        lineHint,
        orderHint: query.orderHint ?? 0,
      });
    } catch (error) {
      if (error instanceof SymbolResolutionError) {
        return attachRawResponseChars(
          {
            status: 'empty',
            errorType: 'symbol_not_found',
            errorCode: LSP_ERROR_CODES.SYMBOL_NOT_FOUND,
            error: error.message,
            hints: [
              ...getHints(TOOL_NAME, 'empty'),
              `Symbol '${symbolName}' not found at line ${lineHint}`,
              'Verify the exact function name (case-sensitive)',
              'Check the line number is correct',
              'Use localSearchCode to find the function first',
            ],
          },
          content.length
        );
      }
      throw error;
    }

    const workspaceRoot = await resolveWorkspaceRootForFile(absolutePath);
    const lspAvailable = await isLanguageServerAvailable(
      absolutePath,
      workspaceRoot
    );

    // No language server: there is no semantic call graph to return. We do
    // NOT guess one from regex/text matching — that would masquerade as a
    // real call hierarchy. Return a clear empty result pointing at the
    // text-based tools instead.
    if (!lspAvailable) {
      return attachRawResponseChars(
        buildLspUnavailableResult(query),
        content.length
      );
    }

    let result: CallHierarchyResult | null = null;
    try {
      result = await callHierarchyWithLSP(
        absolutePath,
        workspaceRoot,
        resolvedSymbol.position,
        query,
        content
      );
    } catch {
      result = null;
    }

    if (!result) {
      return attachRawResponseChars(
        buildLspUnavailableResult(query, true),
        content.length
      );
    }

    // Output bounding is owned entirely by the bulk char-paginator, which
    // sub-slices an oversized node's nested `content` — so no per-tool
    // pre-clip is needed (lossless: the cursor reaches the rest).
    return attachRawResponseChars(
      result,
      content.length + countSerializedChars(result)
    );
  } catch (error) {
    return createErrorResult(error, query, {
      toolName: TOOL_NAME,
    }) as CallHierarchyResult;
  }
}

/**
 * Build the empty result returned when no language server can resolve the
 * call hierarchy. There is no regex/text fallback: a semantic call graph
 * cannot be faithfully reconstructed from text matching, so rather than
 * return misleading guesses we point the caller at the text-search tools.
 *
 * @param lspFailed true when a language server was available but the request
 *   failed or returned nothing (vs. no language server installed at all).
 */
function buildLspUnavailableResult(
  query: LSPCallHierarchyQuery,
  lspFailed = false
): CallHierarchyResult {
  return {
    status: 'empty',
    errorType: 'unknown',
    errorCode: lspFailed
      ? LSP_ERROR_CODES.LSP_EMPTY
      : LSP_ERROR_CODES.LSP_NOT_INSTALLED,
    direction: query.direction,
    depth: query.depth ?? 1,
    hints: [
      ...getHints(TOOL_NAME, 'empty'),
      lspFailed
        ? 'The language server returned no call hierarchy for this symbol.'
        : LSP_UNAVAILABLE_HINT,
      'Use localSearchCode to find callers/callees by text, then localGetFileContent to inspect them.',
    ],
  };
}

/**
 * When `verbosity:"concise"` is requested, drop tree node content and emit
 * graph edges only. Omitted / `"basic"` / `"compact"` behave identically
 * to today.
 *
 * Exported for direct unit testing in `tests/scheme/verbosity_concise.test.ts`.
 */
/** Call-hierarchy edge item shape used to render the concise edge list. */
type ConciseEdgeItem = {
  from?: { name?: string; uri?: string; range?: { start?: { line?: number } } };
  to?: { name?: string; uri?: string; range?: { start?: { line?: number } } };
  fromRanges?: Array<{ start?: { line?: number } }>;
};

function stripConciseItemContent<T>(item: T): T {
  if (!item || typeof item !== 'object') return item;
  const { content: _content, ...rest } = item as T & { content?: unknown };
  return rest as T;
}

function stripConciseCallContent<T extends ConciseEdgeItem>(call: T): T {
  return {
    ...call,
    ...(call.from ? { from: stripConciseItemContent(call.from) } : {}),
    ...(call.to ? { to: stripConciseItemContent(call.to) } : {}),
  };
}

function stripIncomingCallContent(call: IncomingCall): IncomingCall {
  return stripConciseCallContent(call);
}

function stripOutgoingCallContent(call: OutgoingCall): OutgoingCall {
  return stripConciseCallContent(call);
}

/** Render `caller → root` / `root → callee` edge strings for concise output. */
function buildConciseEdges(
  items: ConciseEdgeItem[],
  direction: 'incoming' | 'outgoing',
  rootName: string
): string[] {
  return items.map(item => {
    const peer = direction === 'incoming' ? item.from : item.to;
    const peerName = peer?.name ?? '?';
    const callSites = item.fromRanges?.length ?? 1;
    const suffix = callSites > 1 ? ` (×${callSites})` : '';
    return direction === 'incoming'
      ? `${peerName} → ${rootName}${suffix}`
      : `${rootName} → ${peerName}${suffix}`;
  });
}

/** Collapse a call-hierarchy result to the tiny concise summary form. */
function buildConciseCallHierarchy(
  result: CallHierarchyResult,
  query: LSPCallHierarchyQuery
): CallHierarchyResult {
  const direction = (result.direction ?? query.direction ?? 'incoming') as
    | 'incoming'
    | 'outgoing';
  const root = (result.root ?? (result as { item?: unknown }).item) as
    | { symbol?: { name?: string }; name?: string }
    | undefined;
  const rootName = root?.symbol?.name ?? root?.name ?? query.symbolName ?? '?';
  // The LSP path emits `incomingCalls` / `outgoingCalls`; `calls` is a legacy
  // edge-list field name. Treat all three as the same edge list.
  const calls = (result as { calls?: ConciseEdgeItem[] }).calls;
  const incomingCalls = (result as { incomingCalls?: IncomingCall[] })
    .incomingCalls;
  const outgoingCalls = (result as { outgoingCalls?: OutgoingCall[] })
    .outgoingCalls;
  const items = calls ?? incomingCalls ?? outgoingCalls ?? [];

  const edges = buildConciseEdges(items, direction, rootName);
  const summary = `${edges.length} ${direction} edge(s) for ${rootName} at depth=${result.depth ?? query.depth ?? 1}`;

  // Preserve whichever edge-list field the upstream result used so the
  // output schema validation still passes (the LSP path emits
  // `incomingCalls` / `outgoingCalls`; `calls` is the legacy field name).
  const hasCalls = 'calls' in (result as object);
  const hasIncoming = 'incomingCalls' in (result as object);
  const hasOutgoing = 'outgoingCalls' in (result as object);
  const item =
    result.item && typeof result.item === 'object'
      ? { ...result.item, content: '' }
      : result.item;
  // Drop only char output pagination: it was computed from the full payload
  // before content fields were stripped. Item pagination stays valid because
  // the call arrays are preserved.
  const rest = { ...result } as Record<string, unknown>;
  delete rest.outputPagination;
  return {
    ...(rest as CallHierarchyResult),
    ...(item ? { item: stripConciseItemContent(item) } : {}),
    ...(hasCalls && calls ? { calls: calls.map(stripConciseCallContent) } : {}),
    ...(hasIncoming
      ? { incomingCalls: (incomingCalls ?? []).map(stripIncomingCallContent) }
      : {}),
    ...(hasOutgoing
      ? { outgoingCalls: (outgoingCalls ?? []).map(stripOutgoingCallContent) }
      : {}),
    hints: [summary, `edges: ${edges.join('; ')}`],
  };
}

export function applyCallHierarchyVerbosity(
  result: CallHierarchyResult,
  query: LSPCallHierarchyQuery
): CallHierarchyResult {
  if (isCompact(query)) {
    return {
      ...result,
      hints: compactTrimHints(result.hints, isAdvisoryCallHierarchyHint, 2),
    };
  }
  if (!isConcise(query)) return result;
  if (result.status !== undefined) return result;

  return buildConciseCallHierarchy(result, query);
}
