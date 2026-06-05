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
import type { z } from 'zod';
import type { LSPCallHierarchyQuerySchema } from '@octocodeai/octocode-core/schemas';

type UpstreamLSPCallHierarchyQuery = z.infer<
  typeof LSPCallHierarchyQuerySchema
>;
import type { WithVerbosity } from '../../scheme/localSchemaOverlay.js';
import type { WithOptionalMeta } from '../../types/execution.js';
import { isVerbose } from '../../scheme/verbosity.js';

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
 * Wraps the internal core logic with the verbosity transformer.
 * verbose=false (default): graph edges only (no per-node snippets).
 * verbose=true: full per-node content included.
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
 * Verbosity shaping for lspCallHierarchy.
 *
 * verbose=false (default): omit `lspMode` metadata from results.
 * verbose=true: include all fields including LSP mode info.
 *
 * Calls are never dropped. Hints are always returned fully.
 */
export function applyCallHierarchyVerbosity(
  result: CallHierarchyResult,
  query: LSPCallHierarchyQuery
): CallHierarchyResult {
  if (isVerbose(query) || result.status !== undefined) return result;
  if (!('lspMode' in (result as object))) return result;
  const { lspMode: _lm, ...rest } = result as typeof result & { lspMode?: unknown };
  void _lm;
  return rest as CallHierarchyResult;
}
