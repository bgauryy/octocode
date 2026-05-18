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
import type { LSPCallHierarchyQuery as UpstreamLSPCallHierarchyQuery } from '@octocodeai/octocode-core';
import type { Verbosity } from '../../scheme/localSchemaOverlay.js';
import type { WithOptionalMeta } from '../../types/execution.js';
import { isUltra, ultraDrillBackHint } from '../../scheme/verbosity.js';

type LSPCallHierarchyQuery = WithOptionalMeta<UpstreamLSPCallHierarchyQuery> & {
  verbosity?: Verbosity;
  orderHint?: number;
};
import { ToolErrors } from '../../errors/errorFactories.js';
import { callHierarchyWithLSP } from './callHierarchyLsp.js';
import { callHierarchyWithPatternMatching } from './callHierarchyPatterns.js';
import { applyOutputSizeLimit } from '../../utils/pagination/outputSizeLimit.js';
import { serializeForPagination } from '../../utils/pagination/core.js';
import { TOOL_NAME } from './constants.js';
import { resolveWorkspaceRootForFile } from '../../lsp/workspaceRoot.js';
import { applyQueryOutputPagination } from '../../utils/response/structuredPagination.js';
import { LSP_ERROR_CODES } from '../../lsp/lspErrorCodes.js';
import {
  attachRawResponseChars,
  countSerializedChars,
  getRawResponseChars,
} from '../../utils/response/charSavings.js';

/**
 * Process a single call hierarchy query.
 *
 * Wraps the internal core logic with the RFC §4.7.7 verbosity transformer
 * so that `verbosity:"ultra"` returns graph edges only (no per-node content).
 */
export async function processCallHierarchy(
  query: LSPCallHierarchyQuery
): Promise<CallHierarchyResult> {
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

    let content: string;
    try {
      content = await readFile(absolutePath, 'utf-8');
    } catch (error) {
      const toolError = ToolErrors.fileAccessFailed(
        query.uri,
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
        symbolName: query.symbolName,
        lineHint: query.lineHint,
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
              `Symbol '${query.symbolName}' not found at line ${query.lineHint}`,
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
          const semanticResult = { ...result, lspMode: 'semantic' } as const;
          return attachRawResponseChars(
            applyCallHierarchyOutputLimit(semanticResult, query),
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
    return attachRawResponseChars(
      applyCallHierarchyOutputLimit(withMode, query),
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
 * Apply output size limits to a call hierarchy result.
 * Serializes the result, checks against MAX_OUTPUT_CHARS, and auto-paginates
 * or applies explicit charOffset/charLength if provided.
 */
function applyCallHierarchyOutputLimit(
  result: CallHierarchyResult,
  query: LSPCallHierarchyQuery
): CallHierarchyResult {
  if (result.status !== 'hasResults') return result;

  const serialized = serializeForPagination(result, true);
  const sizeLimitResult = applyOutputSizeLimit(serialized, {
    charOffset: query.charOffset,
    charLength: query.charLength,
  });

  if (!sizeLimitResult.wasLimited || !sizeLimitResult.pagination) return result;

  // CallHierarchyResult satisfies Record<string, unknown> via its [key: string]:
  // unknown index signature inherited from LSPToolResultBase, so no cast needed.
  const pagedQueryResult = applyQueryOutputPagination(
    {
      id: query.id ?? 'q1',
      status: result.status,
      data: result,
    },
    {
      charOffset: sizeLimitResult.pagination.charOffset,
      charLength: sizeLimitResult.pagination.charLength,
    },
    TOOL_NAME
  );

  // pagedQueryResult.data is Record<string, unknown> — narrow what we read
  // back into typed values rather than asserting a Partial<CallHierarchyResult>.
  const pagedData = pagedQueryResult.data;
  const pagedHints = Array.isArray(pagedData.hints)
    ? pagedData.hints.filter((h): h is string => typeof h === 'string')
    : [];

  // Re-spread the original result.hints to make hint-preservation an explicit
  // invariant. applyQueryOutputPagination today excludes 'hints' from
  // structured pagination, so result.hints survives in pagedData.hints — but
  // re-spreading guards against future paginator changes that might paginate
  // hints, and aligns with applyGotoDefinitionOutputLimit's pattern. Set
  // dedupes the outer hints with whatever pagedData.hints carries through.
  const combinedHints = [
    ...(result.hints ?? []),
    ...pagedHints,
    ...sizeLimitResult.warnings,
    ...sizeLimitResult.paginationHints,
  ];

  const pagedLspMode: CallHierarchyResult['lspMode'] =
    pagedData.lspMode === 'semantic' || pagedData.lspMode === 'fallback'
      ? pagedData.lspMode
      : undefined;

  return {
    ...result,
    ...pagedData,
    // Pin lspMode from the pre-pagination result. pagedData may omit it
    // if the JSON slice lands past the field, and a slice that explicitly
    // serialised `lspMode` to undefined would otherwise erase the marker.
    lspMode: result.lspMode ?? pagedLspMode,
    outputPagination: {
      charOffset: sizeLimitResult.pagination.charOffset!,
      charLength: sizeLimitResult.pagination.charLength!,
      totalChars: sizeLimitResult.pagination.totalChars!,
      hasMore: sizeLimitResult.pagination.hasMore,
      currentPage: sizeLimitResult.pagination.currentPage,
      totalPages: sizeLimitResult.pagination.totalPages,
    },
    hints: Array.from(new Set(combinedHints)),
  };
}

/**
 * RFC §4.7.7: when `verbosity:"ultra"` is requested, drop tree node content
 * and emit graph edges only. Compact / verbose / omitted behave identically
 * to today.
 *
 * Exported for direct unit testing in `tests/scheme/verbosity_ultra.test.ts`.
 */
export function applyCallHierarchyVerbosity(
  result: CallHierarchyResult,
  query: LSPCallHierarchyQuery
): CallHierarchyResult {
  if (!isUltra(query.verbosity)) return result;
  if (result.status === 'error') return result;

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
    []) as Array<{
    from?: {
      name?: string;
      uri?: string;
      range?: { start?: { line?: number } };
    };
    to?: { name?: string; uri?: string; range?: { start?: { line?: number } } };
    fromRanges?: Array<{ start?: { line?: number } }>;
  }>;

  const edges = items.map(item => {
    const peer = direction === 'incoming' ? item.from : item.to;
    const peerName = peer?.name ?? '?';
    const callSites = item.fromRanges?.length ?? 1;
    return direction === 'incoming'
      ? `${peerName} → ${rootName}${callSites > 1 ? ` (×${callSites})` : ''}`
      : `${rootName} → ${peerName}${callSites > 1 ? ` (×${callSites})` : ''}`;
  });

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
  return {
    ...result,
    ...(item ? { item } : {}),
    ...(hasCalls ? { calls: [] } : {}),
    ...(hasIncoming ? { incomingCalls: [] } : {}),
    ...(hasOutgoing ? { outgoingCalls: [] } : {}),
    hints: [
      summary,
      `edges: ${edges.join('; ')}`,
      ...ultraDrillBackHint(
        're-call with verbosity:"compact" (default) for full per-node context'
      ),
    ],
  };
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
