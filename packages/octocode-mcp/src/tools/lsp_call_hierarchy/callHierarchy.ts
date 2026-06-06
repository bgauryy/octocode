import { readFile } from 'fs/promises';
import { getHints } from '../../hints/index.js';
import { hints as callHierarchyHints } from './hints.js';
import {
  validateToolPath,
  createErrorResult,
} from '../../utils/file/toolHelpers.js';
import { SymbolResolver, SymbolResolutionError } from '../../lsp/resolver.js';
import { isLanguageServerAvailable } from '../../lsp/manager.js';
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

export async function processCallHierarchy(
  query: LSPCallHierarchyQuery
): Promise<CallHierarchyResult> {
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
              `Symbol '${symbolName}' not found at line ${lineHint} — lineHint is likely stale (file changed since the line was recorded).`,
              'Re-anchor: run localSearchCode with the exact symbol name to get the current line number, then retry.',
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

function buildLspUnavailableResult(
  query: LSPCallHierarchyQuery,
  lspFailed = false
): CallHierarchyResult {
  return {
    status: 'empty',
    errorType: 'lsp_unavailable',
    errorCode: lspFailed
      ? LSP_ERROR_CODES.LSP_EMPTY
      : LSP_ERROR_CODES.LSP_NOT_INSTALLED,
    direction: query.direction,
    depth: query.depth ?? 1,
    hints: callHierarchyHints
      .error({
        errorType: 'lsp_unavailable',
        symbolName: query.symbolName,
      })
      .filter((hint): hint is string => typeof hint === 'string'),
  };
}

export function applyCallHierarchyVerbosity(
  result: CallHierarchyResult,
  query: LSPCallHierarchyQuery
): CallHierarchyResult {
  if (isVerbose(query)) return result;
  if (result.status !== undefined) return result;

  const r = result as typeof result & { lspMode?: unknown };

  const incomingCalls = r.incomingCalls?.map(call => ({
    ...call,
    from: stripContent(call.from),
  }));
  const outgoingCalls = r.outgoingCalls?.map(call => ({
    ...call,
    to: stripContent(call.to),
  }));

  const callerCount =
    (incomingCalls?.length ?? 0) + (outgoingCalls?.length ?? 0);
  const fileCount = new Set([
    ...(incomingCalls?.map(c => c.from.uri) ?? []),
    ...(outgoingCalls?.map(c => c.to.uri) ?? []),
  ]).size;

  const { lspMode: _lm, ...rest } = r;
  void _lm;

  return {
    ...(rest as CallHierarchyResult),
    ...(incomingCalls !== undefined ? { incomingCalls } : {}),
    ...(outgoingCalls !== undefined ? { outgoingCalls } : {}),
    summary: { callerCount, fileCount },
  };
}

function stripContent<T extends { content?: string }>(
  item: T
): Omit<T, 'content'> {
  const { content: _c, ...rest } = item;
  void _c;
  return rest as Omit<T, 'content'>;
}
