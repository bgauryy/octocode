import { readFile, stat } from 'fs/promises';

import type { z } from 'zod';
import type { LSPFindReferencesQuerySchema } from '@octocodeai/octocode-core/schemas';

type UpstreamLSPFindReferencesQuery = z.infer<
  typeof LSPFindReferencesQuerySchema
>;
import type { WithVerbosity } from '../../scheme/localSchemaOverlay.js';
import type { WithOptionalMeta } from '../../types/execution.js';
import { isVerbose } from '../../scheme/verbosity.js';

type LSPFindReferencesQuery = WithVerbosity<
  WithOptionalMeta<UpstreamLSPFindReferencesQuery>
> & {
  groupByFile?: boolean;
  orderHint?: number;
};
import { SymbolResolver, SymbolResolutionError } from '../../lsp/resolver.js';
import { isLanguageServerAvailable } from '../../lsp/manager.js';
import type {
  FindReferencesResult,
  ExactPosition,
  ReferenceLocation,
  ReferencesByFile,
} from '../../lsp/types.js';
import {
  validateToolPath,
  createErrorResult,
} from '../../utils/file/toolHelpers.js';
import { ToolErrors } from '../../errors/errorFactories.js';
import { getHints } from '../../hints/index.js';
import { TOOL_NAME } from './constants.js';
import { findReferencesWithLSP } from './lspReferencesCore.js';
import { resolveWorkspaceRootForFile } from '../../lsp/workspaceRoot.js';
import { LSP_ERROR_CODES } from '../../lsp/lspErrorCodes.js';
import {
  attachRawResponseChars,
  countSerializedChars,
  getRawResponseChars,
} from '../../utils/response/charSavings.js';
import { attachLspEvidence } from '../../lsp/evidence.js';

export async function findReferences(
  query: LSPFindReferencesQuery
): Promise<FindReferencesResult> {
  const bridge = query as {
    itemsPerPage?: number;
    referencesPerPage?: number;
  };
  if (
    bridge.referencesPerPage === undefined &&
    typeof bridge.itemsPerPage === 'number'
  ) {
    bridge.referencesPerPage = bridge.itemsPerPage;
  }
  const result = await findReferencesInternal(query);
  const rawChars = getRawResponseChars(result) ?? countSerializedChars(result);
  const shaped = attachReferencesEvidence(
    applyFindReferencesVerbosity(result, query)
  );
  return attachRawResponseChars(shaped, rawChars);
}

function attachReferencesEvidence(
  result: FindReferencesResult
): FindReferencesResult {
  return attachLspEvidence(result, {
    kind: 'references',
    paginationKey: 'pagination',
  });
}

async function findReferencesInternal(
  query: LSPFindReferencesQuery
): Promise<FindReferencesResult> {
  try {
    const pathValidation = validateToolPath(
      { ...query, path: query.uri },
      TOOL_NAME
    );
    if (!pathValidation.isValid) {
      return pathValidation.errorResult as FindReferencesResult;
    }

    const absolutePath = pathValidation.sanitizedPath!;
    const uri = query.uri!;
    const symbolName = query.symbolName!;
    const lineHint = query.lineHint!;

    try {
      await stat(absolutePath);
    } catch (error) {
      const toolError = ToolErrors.fileAccessFailed(
        uri,
        error instanceof Error ? error : undefined
      );
      return createErrorResult(toolError, query, {
        toolName: TOOL_NAME,
        extra: { resolvedPath: absolutePath },
      }) as FindReferencesResult;
    }

    let content: string;
    try {
      content = await readFile(absolutePath, 'utf-8');
    } catch (error) {
      const toolError = ToolErrors.fileReadFailed(
        uri,
        error instanceof Error ? error : undefined
      );
      return createErrorResult(toolError, query, {
        toolName: TOOL_NAME,
        extra: { resolvedPath: absolutePath },
      }) as FindReferencesResult;
    }

    const resolver = new SymbolResolver({ lineSearchRadius: 5 });
    let resolvedSymbol: { position: ExactPosition; foundAtLine: number };
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
            error: error.message,
            errorType: 'symbol_not_found',
            errorCode: LSP_ERROR_CODES.SYMBOL_NOT_FOUND,
            hints: [
              `Symbol '${symbolName}' not found at or near line ${lineHint} — lineHint is likely stale (file changed since the line was recorded).`,
              `Searched +/-${error.searchRadius} lines from line ${lineHint}`,
              'Re-anchor: run localSearchCode with the exact symbol name to get the current line number, then retry with that lineHint.',
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
        buildLspUnavailableResult(false, query.symbolName),
        content.length
      );
    }

    const lspQuery: LSPFindReferencesQuery = query.groupByFile
      ? { ...query, page: 1, referencesPerPage: Number.MAX_SAFE_INTEGER }
      : query;

    let lspResult: FindReferencesResult | null = null;
    try {
      lspResult = await findReferencesWithLSP(
        absolutePath,
        workspaceRoot,
        resolvedSymbol.position,
        lspQuery
      );
    } catch {
      lspResult = null;
    }

    if (!lspResult) {
      return attachRawResponseChars(
        buildLspUnavailableResult(true, query.symbolName),
        content.length
      );
    }

    return attachRawResponseChars(
      lspResult,
      content.length + countSerializedChars(lspResult)
    );
  } catch (error) {
    return createErrorResult(error, query, {
      toolName: TOOL_NAME,
    }) as FindReferencesResult;
  }
}

function buildLspUnavailableResult(
  lspFailed = false,
  symbolName?: string
): FindReferencesResult {
  return {
    status: 'empty',
    errorType: 'lsp_unavailable',
    errorCode: lspFailed
      ? LSP_ERROR_CODES.LSP_EMPTY
      : LSP_ERROR_CODES.LSP_NOT_INSTALLED,
    hints: [
      ...getHints(TOOL_NAME, 'empty'),
      ...getHints(TOOL_NAME, 'error', {
        errorType: 'lsp_unavailable',
        symbolName,
      }),
    ],
  };
}

function buildReferencesByFile(
  locations: readonly ReferenceLocation[]
): ReferencesByFile[] {
  const byUri = new Map<string, ReferencesByFile>();

  for (const loc of locations) {
    const lineNumber = loc.range.start.line + 1;
    const existing = byUri.get(loc.uri);
    if (existing) {
      const hasDefinition = existing.hasDefinition || loc.isDefinition;
      existing.count += 1;
      existing.lines.push(lineNumber);
      if (hasDefinition) existing.hasDefinition = true;
      continue;
    }

    byUri.set(loc.uri, {
      uri: loc.uri,
      count: 1,
      firstLine: lineNumber,
      firstCharacter: loc.range.start.character,
      lines: [lineNumber],
      ...(loc.isDefinition ? { hasDefinition: true } : {}),
    });
  }

  return [...byUri.values()].sort((left, right) => {
    const countDelta = right.count - left.count;
    if (countDelta !== 0) return countDelta;
    return left.uri.localeCompare(right.uri);
  });
}

export function applyFindReferencesVerbosity(
  result: FindReferencesResult,
  query: LSPFindReferencesQuery
): FindReferencesResult {
  if (result.status !== undefined || !result.locations?.length) return result;

  if (query.groupByFile) {
    const byFile = buildReferencesByFile(result.locations);
    const summary = `${result.locations.length} refs in ${byFile.length} files`;
    return {
      ...result,
      locations: [],
      byFile,
      totalReferences: result.locations.length,
      totalFiles: byFile.length,
      hints: [summary],
    };
  }

  if (isVerbose(query)) return result;
  if (!('lspMode' in (result as object))) return result;
  const { lspMode: _lm, ...rest } = result as typeof result & {
    lspMode?: unknown;
  };
  void _lm;
  return rest as FindReferencesResult;
}

export { findReferencesWithLSP } from './lspReferencesCore.js';
