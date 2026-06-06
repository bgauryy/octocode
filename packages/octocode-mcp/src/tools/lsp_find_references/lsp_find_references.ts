/**
 * LSP Find References Tool
 *
 * Finds all references to a symbol across the workspace using Language Server Protocol.
 *
 * @module tools/lsp_find_references
 */

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

/**
 * Find all references to a symbol.
 *
 * Wraps the internal core logic with the verbosity transformer.
 * verbose=false (default): compact result set (location strings, no snippets).
 * verbose=true: full per-reference content included.
 */
export async function findReferences(
  query: LSPFindReferencesQuery
): Promise<FindReferencesResult> {
  // Surface page-size knob is the cross-tool `itemsPerPage`; the internal
  // pipeline threads `referencesPerPage`. Bridge once here so all downstream
  // logic (resolveReferencePagination, core/patterns builders) is unchanged.
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
  // Row navigation is page-based (`page` / `referencesPerPage`). The generic
  // bulk response does not expose response-level char cursors, so this tool
  // returns the current page in full and relies on item pagination for bounds.
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
              `Symbol '${symbolName}' not found at or near line ${lineHint}`,
              `Searched +/-${error.searchRadius} lines from line ${lineHint}`,
              'Verify the exact symbol name (case-sensitive, no partial matches)',
              'Use localGetFileContent to check the file content around that line',
              'Use localSearchCode to find the correct line number first',
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

    // No language server: we do NOT fall back to regex/text matching dressed
    // up as semantic references (it misses renamed/aliased usages and yields
    // false positives). Return a clear empty result that routes the caller to
    // the dedicated text-search tool instead.
    if (!lspAvailable) {
      return attachRawResponseChars(
        buildLspUnavailableResult(false, query.symbolName),
        content.length
      );
    }

    // groupByFile is a full-set rollup: the per-file map must aggregate the
    // COMPLETE reference set, so fetch every reference on one page. Flat /
    // snippet modes page normally via the caller's page/referencesPerPage.
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

/**
 * Build the empty result returned when no language server can resolve
 * references. There is no regex/text fallback: text matching cannot
 * distinguish semantic references from incidental name collisions, so rather
 * than return misleading guesses we point the caller at the text-search tool.
 *
 * @param lspFailed true when a language server was available but the request
 *   failed or returned nothing (vs. no language server installed at all).
 */
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

/**
 * Adaptive concise threshold. Below this fanout the response is a
 * flat `refs[]` of "file:line" strings (still fits one 8 KB page); at or above
 * it the response auto-degrades to a `byFile` rollup so the payload is
 * bounded regardless of fanout. Validated by `measure.mjs::demo9` (≤ 443
 * chars at 10,000 refs).
 */

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

/**
 * Verbosity shaping for lspFindReferences.
 *
 * groupByFile: short-circuits verbosity — returns per-file counts regardless.
 * verbose=false (default): omit `lspMode` metadata from results.
 * verbose=true: include all fields.
 *
 * Locations are never dropped. Hints are always returned fully.
 */
export function applyFindReferencesVerbosity(
  result: FindReferencesResult,
  query: LSPFindReferencesQuery
): FindReferencesResult {
  if (result.status !== undefined || !result.locations?.length) return result;

  // groupByFile is a product mode — short-circuits verbosity.
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
