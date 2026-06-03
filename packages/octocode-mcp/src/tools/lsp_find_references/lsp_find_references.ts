/**
 * LSP Find References Tool
 *
 * Finds all references to a symbol across the workspace using Language Server Protocol.
 * Falls back to pattern matching when LSP is not available.
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
import {
  isConcise,
  isCompact,
  compactTrimHints,
  makeAdvisoryPredicate,
} from '../../scheme/verbosity.js';

/** Advisory hints lspFindReferences emits; stripped under compact.
 * Substring-OR, case-insensitive. */
const isAdvisoryFindReferencesHint = makeAdvisoryPredicate([
  'groupbyfile',
  'includepattern',
  'excludepattern',
  'fallback',
  'impact analysis',
]);

type LSPFindReferencesQuery = WithVerbosity<
  WithOptionalMeta<UpstreamLSPFindReferencesQuery>
> & {
  groupByFile?: boolean;
  orderHint?: number;
};
import { SymbolResolver, SymbolResolutionError } from '../../lsp/resolver.js';
import {
  isLanguageServerAvailable,
  LSP_UNAVAILABLE_HINT,
} from '../../lsp/manager.js';
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
 * Wraps the internal core logic with the verbosity transformer so that
 * `verbosity:"concise"` shrinks the payload to a flat `refs[]` array of
 * `file:line` strings (≤ 500 refs) or a `byFile` rollup (≥ 500 refs).
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
  // Output bounding for this tool is handled exclusively by applyBulkResponsePagination
  // (the query-level applyQueryOutputPagination is bypassed — see the early-return
  // guard in structuredPagination.ts). The LSP_FIND_REFERENCES case in
  // structuredPagination.ts char-paginates the `locations` array, slicing it
  // to the responseCharLength budget and sub-slicing an oversized single
  // location's `content`. Row navigation stays on `page` / `referencesPerPage`;
  // bulk responseCharOffset / responseCharLength are the only cursor levers.
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
        buildLspUnavailableResult(),
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
        buildLspUnavailableResult(true),
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
function buildLspUnavailableResult(lspFailed = false): FindReferencesResult {
  return {
    status: 'empty',
    errorType: 'unknown',
    errorCode: lspFailed
      ? LSP_ERROR_CODES.LSP_EMPTY
      : LSP_ERROR_CODES.LSP_NOT_INSTALLED,
    hints: [
      ...getHints(TOOL_NAME, 'empty'),
      lspFailed
        ? 'The language server returned no references for this symbol.'
        : LSP_UNAVAILABLE_HINT,
      'Use localSearchCode to find textual usages of the symbol across the workspace.',
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
const CONCISE_REFS_FLAT_THRESHOLD = 500;

function buildReferencesByFile(
  locations: readonly ReferenceLocation[]
): ReferencesByFile[] {
  const byUri = new Map<string, ReferencesByFile>();

  for (const loc of locations) {
    const existing = byUri.get(loc.uri);
    if (existing) {
      const hasDefinition = existing.hasDefinition || loc.isDefinition;
      byUri.set(loc.uri, {
        ...existing,
        count: existing.count + 1,
        ...(hasDefinition ? { hasDefinition: true } : {}),
      });
      continue;
    }

    byUri.set(loc.uri, {
      uri: loc.uri,
      count: 1,
      firstLine: loc.range.start.line + 1,
      firstCharacter: loc.range.start.character,
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
 * Shape the response according to `verbosity` / `groupByFile`. Omitted /
 * `"basic"` / `"compact"` preserve full results; concise is
 * lossy by design and carries an explicit drill-back hint.
 */
export function applyFindReferencesVerbosity(
  result: FindReferencesResult,
  query: LSPFindReferencesQuery
): FindReferencesResult {
  if (result.status !== undefined || !result.locations?.length) return result;

  // groupByFile is a tier-orthogonal product mode — short-circuits the
  // verbosity switch regardless of basic/compact/concise.
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

  if (isCompact(query)) {
    return {
      ...result,
      hints: compactTrimHints(result.hints, isAdvisoryFindReferencesHint, 2),
    };
  }

  if (!isConcise(query)) return result;

  const refs = result.locations.map(
    loc => `${loc.uri}:${loc.range.start.line + 1}`
  );
  const uniqueFiles = new Set(result.locations.map(l => l.uri));

  if (refs.length < CONCISE_REFS_FLAT_THRESHOLD) {
    const summary = `${refs.length} refs in ${uniqueFiles.size} files`;
    return {
      ...result,
      locations: [],
      hints: [summary, `refs: ${refs.join(', ')}`],
    };
  }

  const byFile: Record<string, number> = {};
  for (const loc of result.locations) {
    byFile[loc.uri] = (byFile[loc.uri] ?? 0) + 1;
  }
  const topFiles = Object.entries(byFile)
    .sort(([, a], [, b]) => b - a)
    .slice(0, 20);
  const topFilesStr = topFiles.map(([f, n]) => `${f}(${n})`).join(', ');
  const summary =
    `${refs.length} refs in ${uniqueFiles.size} files; ` +
    `top-20: ${topFilesStr}`;

  return {
    ...result,
    locations: [],
    hints: [summary],
  };
}

export { findReferencesWithLSP } from './lspReferencesCore.js';
