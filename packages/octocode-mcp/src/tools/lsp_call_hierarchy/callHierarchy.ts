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
import type { CallHierarchyResult } from '../../lsp/types.js';
import type { z } from 'zod/v4';
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
import { callHierarchyWithPatternMatching } from './callHierarchyPatterns.js';
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

    let semanticFallbackHint: string | undefined;
    if (lspAvailable) {
      try {
        const result = await callHierarchyWithLSP(
          absolutePath,
          workspaceRoot,
          resolvedSymbol.position,
          query,
          content
        );
        if (result) {
          // Semantic path: omit lspMode (absent ≡ semantic). Fallback path
          // below explicitly sets lspMode='fallback'.
          const semanticResult = result;
          return attachRawResponseChars(
            // Output bounding is owned entirely by the bulk char-paginator,
            // which sub-slices an oversized node's nested `content` — so no
            // per-tool pre-clip is needed (lossless: the cursor reaches the rest).
            semanticResult,
            content.length + countSerializedChars(semanticResult)
          );
        }
        semanticFallbackHint =
          'LSP semantic call hierarchy returned no result; using text fallback';
      } catch {
        semanticFallbackHint =
          'LSP semantic call hierarchy failed; using text fallback';
      }
    }

    const patternResult = await callHierarchyWithPatternMatching(
      query,
      absolutePath,
      workspaceRoot,
      content,
      resolvedSymbol.foundAtLine,
      resolver
    );
    // The published output schema is closed for status='error' — `lspMode`
    // is only valid on hasResults/empty. Skip the tag on error responses
    // to keep MCP schema validation passing. We still surface the
    // LSP-unavailable hint through hints[].
    const withMode: CallHierarchyResult =
      patternResult.status === 'error'
        ? withLspUnavailableHint(
            patternResult,
            lspAvailable,
            semanticFallbackHint
          )
        : {
            ...withLspUnavailableHint(
              patternResult,
              lspAvailable,
              semanticFallbackHint
            ),
            lspMode: 'fallback',
          };
    // Char-pagination is owned by the unified bulk engine:
    // applyQueryOutputPagination (explicit charOffset/charLength) and
    // applyBulkResponsePagination (auto-cap) handle it via the same single
    // getOutputCharLimit() flow as every other tool. The old per-tool
    // applyCallHierarchyOutputLimit pre-paginated against JSON length and
    // emitted its own outputPagination + "Auto-paginated" breadcrumbs
    // ALONGSIDE the bulk layer — producing contradictory totals (three
    // different char counts for the same query). Removing it here means the
    // bulk engine is the one and only pagination authority.
    return attachRawResponseChars(
      withMode,
      content.length + countSerializedChars(withMode)
    );
  } catch (error) {
    return createErrorResult(error, query, {
      toolName: TOOL_NAME,
    }) as CallHierarchyResult;
  }
}

/**
 * Prepend the shared LSP-unavailable hint when the result came from the
 * pattern-matching fallback rather than a real language server. Without
 * this, agents mistake partial text-based matches for semantic call graphs.
 */
function withLspUnavailableHint(
  result: CallHierarchyResult,
  lspAvailable: boolean,
  semanticFallbackHint?: string
): CallHierarchyResult {
  if (semanticFallbackHint) {
    return {
      ...result,
      hints: [semanticFallbackHint, ...(result.hints || [])],
    };
  }
  if (lspAvailable) return result;
  return {
    ...result,
    hints: [LSP_UNAVAILABLE_HINT, ...(result.hints || [])],
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
  // The pattern-fallback emits `incomingCalls` / `outgoingCalls`, the LSP
  // path emits `calls`. Treat all three as the same edge list.
  const items = ((result as { calls?: unknown[] }).calls ??
    (result as { incomingCalls?: unknown[] }).incomingCalls ??
    (result as { outgoingCalls?: unknown[] }).outgoingCalls ??
    []) as ConciseEdgeItem[];

  const edges = buildConciseEdges(items, direction, rootName);
  const summary = `${edges.length} ${direction} edge(s) for ${rootName} at depth=${result.depth ?? query.depth ?? 1}`;

  // Preserve whichever edge-list field the upstream result used so the
  // output schema validation still passes. The LSP path emits `calls`, the
  // pattern-fallback path emits `incomingCalls` / `outgoingCalls`.
  const hasCalls = 'calls' in (result as object);
  const hasIncoming = 'incomingCalls' in (result as object);
  const hasOutgoing = 'outgoingCalls' in (result as object);
  const item =
    result.item && typeof result.item === 'object'
      ? { ...result.item, content: '' }
      : result.item;
  // Concise is a complete, tiny probe answer (the edge list lives in `hints`).
  // Drop pagination / outputPagination: they were computed from the full
  // payload before calls[] was emptied, so they're stale and would both bloat
  // the response and falsely mark it incomplete. (#T3 / #5b)
  const rest = { ...result } as Record<string, unknown>;
  delete rest.pagination;
  delete rest.outputPagination;
  return {
    ...(rest as CallHierarchyResult),
    ...(item ? { item } : {}),
    ...(hasCalls ? { calls: [] } : {}),
    ...(hasIncoming ? { incomingCalls: [] } : {}),
    ...(hasOutgoing ? { outgoingCalls: [] } : {}),
    hints: [summary, `edges: ${edges.join('; ')}`],
  };
}

export function applyCallHierarchyVerbosity(
  result: CallHierarchyResult,
  query: LSPCallHierarchyQuery
): CallHierarchyResult {
  if (isCompact(query.verbosity)) {
    return {
      ...result,
      hints: compactTrimHints(result.hints, isAdvisoryCallHierarchyHint, 2),
    };
  }
  if (!isConcise(query.verbosity)) return result;
  if (result.status !== undefined) return result;

  return buildConciseCallHierarchy(result, query);
}

export {
  parseRipgrepJsonOutput,
  extractFunctionBody,
} from './callHierarchyPatterns.js';
export {
  isFunctionAssignment,
  inferSymbolKind,
  createRange,
  escapeRegex,
} from './callHierarchyHelpers.js';
