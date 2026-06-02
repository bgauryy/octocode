/**
 * LSP Find References Tool
 *
 * Finds all references to a symbol across the workspace using Language Server Protocol.
 * Falls back to pattern matching when LSP is not available.
 *
 * @module tools/lsp_find_references
 */

import { readFile, stat } from 'fs/promises';

import type { z } from 'zod/v4';
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
import { TOOL_NAME } from './constants.js';
import { findReferencesWithLSP } from './lspReferencesCore.js';
import { findReferencesWithPatternMatching } from './lspReferencesPatterns.js';
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
    fallbackReason:
      'Results derived from text pattern matching; may include false positives or miss renamed/aliased usages.',
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
    const globalMergeQuery = createGlobalMergeQuery(query);

    let lspResult: FindReferencesResult | null = null;
    let semanticFallbackHint: string | undefined;
    const lspAvailable = await isLanguageServerAvailable(
      absolutePath,
      workspaceRoot
    );
    if (lspAvailable) {
      try {
        lspResult = await findReferencesWithLSP(
          absolutePath,
          workspaceRoot,
          resolvedSymbol.position,
          globalMergeQuery
        );
      } catch {
        semanticFallbackHint =
          'LSP semantic references failed; using text fallback';
      }
    }

    const patternResult = await findReferencesWithPatternMatching(
      absolutePath,
      workspaceRoot,
      globalMergeQuery
    );

    const lspHasLocations =
      !!lspResult &&
      lspResult.status === undefined &&
      !!lspResult.locations?.length;
    const patternHasLocations =
      patternResult.status === undefined && !!patternResult.locations?.length;

    if (!lspHasLocations) {
      // Pattern-only branch — locations did not come from semantic LSP,
      // even when isLanguageServerAvailable=true (LSP returned empty or
      // threw). Tag fallback so agents do not treat results as
      // authoritative — but only when the result shape allows it.
      // `lspMode` is only valid on hasResults/empty per the published
      // output schema; injecting it on error would fail MCP validation.
      if (lspAvailable && !semanticFallbackHint) {
        semanticFallbackHint =
          'LSP semantic references returned no result; using text fallback';
      }
      const hintedPattern = withLspUnavailableHint(
        patternResult,
        lspAvailable,
        semanticFallbackHint
      );
      const tagged: FindReferencesResult =
        hintedPattern.status === 'error'
          ? hintedPattern
          : { ...hintedPattern, lspMode: 'fallback' };
      return attachRawResponseChars(
        paginateGlobalBranchResult(tagged, query),
        content.length + countSerializedChars(tagged)
      );
    }

    if (!patternHasLocations) {
      // Semantic path: omit lspMode entirely (absent ≡ semantic).
      // Only fallback paths set lspMode='fallback' to flag downgraded resolution.
      const semanticResult = lspResult!;
      return attachRawResponseChars(
        paginateGlobalBranchResult(semanticResult, query),
        content.length + countSerializedChars(semanticResult)
      );
    }

    // Semantic merge path: no lspMode marker (absent ≡ semantic).
    const mergedResult = mergeReferenceResults(lspResult, patternResult, query);
    return attachRawResponseChars(
      mergedResult,
      content.length + countSerializedChars(mergedResult)
    );
  } catch (error) {
    return createErrorResult(error, query, {
      toolName: TOOL_NAME,
    }) as FindReferencesResult;
  }
}

/**
 * Merge LSP and pattern-matching results for comprehensive coverage.
 *
 * LSP provides semantic accuracy but may miss cross-file references on cold start.
 * Pattern matching (ripgrep) provides comprehensive text-based coverage.
 * Merging both gives the best of both worlds without persistent caching.
 *
 * Deduplication is by (uri, startLine) to avoid showing the same reference twice.
 */
export function mergeReferenceResults(
  lspResult: FindReferencesResult | null,
  patternResult: FindReferencesResult,
  query: LSPFindReferencesQuery
): FindReferencesResult {
  if (
    !lspResult ||
    lspResult.status === 'empty' ||
    !lspResult.locations?.length
  ) {
    return patternResult;
  }

  if (patternResult.status === 'empty' || !patternResult.locations?.length) {
    return lspResult;
  }

  const seen = new Set(
    lspResult.locations.map(
      (loc: ReferenceLocation) =>
        `${loc.uri}:${loc.range.start.line}:${loc.range.start.character}`
    )
  );

  const additionalRefs = patternResult.locations.filter(
    (loc: ReferenceLocation) =>
      !seen.has(
        `${loc.uri}:${loc.range.start.line}:${loc.range.start.character}`
      )
  );

  // When pattern matching surfaces nothing beyond the semantic set, the two
  // agree. Record that, but STILL paginate: previously this branch
  // short-circuited and returned `lspResult` verbatim — which was fetched via
  // createGlobalMergeQuery (referencesPerPage = MAX_SAFE_INTEGER), so it
  // silently ignored the caller's page/referencesPerPage and returned every
  // reference in a single page. Falling through to the shared pagination
  // block below fixes that while preserving the "confirmed" hint.
  const allConfirmed = additionalRefs.length === 0;
  const mergedLocations = allConfirmed
    ? [...lspResult.locations]
    : [...lspResult.locations, ...additionalRefs];
  const baseHints = [...(lspResult.hints || [])];
  if (allConfirmed) {
    baseHints.push('All references confirmed by both LSP and text search');
  }
  const totalReferences = mergedLocations.length;
  const uniqueFiles = new Set(
    mergedLocations.map((ref: ReferenceLocation) => ref.uri)
  );

  const { page, referencesPerPage } = resolveReferencePagination(query);
  const totalPages = Math.ceil(totalReferences / referencesPerPage);
  if (totalReferences > 0 && page > totalPages) {
    return {
      status: 'empty',
      pagination: {
        currentPage: page,
        totalPages,
        totalResults: totalReferences,
        hasMore: false,
        ...(referencesPerPage < Number.MAX_SAFE_INTEGER
          ? { resultsPerPage: referencesPerPage }
          : {}),
      },
      hasMultipleFiles: uniqueFiles.size > 1,
      hints: [
        ...baseHints,
        `Requested page ${page} is outside available range (1-${totalPages}).`,
      ],
    };
  }
  const startIndex = (page - 1) * referencesPerPage;
  const endIndex = Math.min(startIndex + referencesPerPage, totalReferences);
  const paginatedLocations = mergedLocations.slice(startIndex, endIndex);

  const hints = [...baseHints];
  if (page < totalPages) {
    hints.push(
      `Showing page ${page} of ${totalPages}. Use page=${page + 1} for more.`
    );
  }

  return {
    locations: paginatedLocations,
    pagination: {
      currentPage: page,
      totalPages,
      totalResults: totalReferences,
      hasMore: page < totalPages,
      ...(referencesPerPage < Number.MAX_SAFE_INTEGER
        ? { resultsPerPage: referencesPerPage }
        : {}),
    },
    hasMultipleFiles: uniqueFiles.size > 1,
    hints,
  };
}

/**
 * Prepend the shared LSP-unavailable hint to the result when no language
 * server could be located. This is the only reliable signal for callers
 * that the returned references come from text search, not semantic
 * analysis, and will therefore miss renamed/aliased usages.
 */
function withLspUnavailableHint(
  result: FindReferencesResult,
  lspAvailable: boolean,
  semanticFallbackHint?: string
): FindReferencesResult {
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
 * Effective row pagination for a references query.
 *
 * `groupByFile` is a full-set blast-radius rollup: the per-file map is the
 * unit of output, not individual references. It must therefore aggregate the
 * COMPLETE reference set — paginating the underlying refs first would make the
 * rollup count only the current page (the F1 regression). So groupByFile
 * disables row pagination; flat/snippet modes page normally.
 */
function resolveReferencePagination(query: LSPFindReferencesQuery): {
  page: number;
  referencesPerPage: number;
} {
  if (query.groupByFile) {
    return { page: 1, referencesPerPage: Number.MAX_SAFE_INTEGER };
  }
  return {
    page: query.page ?? 1,
    referencesPerPage: query.referencesPerPage ?? 20,
  };
}

function createGlobalMergeQuery(
  query: LSPFindReferencesQuery
): LSPFindReferencesQuery {
  return {
    ...query,
    page: 1,
    referencesPerPage: Number.MAX_SAFE_INTEGER,
  };
}

function paginateGlobalBranchResult(
  result: FindReferencesResult,
  query: LSPFindReferencesQuery
): FindReferencesResult {
  if (result.status !== undefined || !result.locations?.length) {
    return result;
  }

  const { page, referencesPerPage } = resolveReferencePagination(query);
  const totalReferences = result.locations.length;
  const totalPages = Math.ceil(totalReferences / referencesPerPage);
  const hasMultipleFiles =
    new Set(result.locations.map(ref => ref.uri)).size > 1;

  if (totalReferences > 0 && page > totalPages) {
    return {
      status: 'empty',
      pagination: {
        currentPage: page,
        totalPages,
        totalResults: totalReferences,
        hasMore: false,
        ...(referencesPerPage < Number.MAX_SAFE_INTEGER
          ? { resultsPerPage: referencesPerPage }
          : {}),
      },
      hasMultipleFiles,
      hints: [
        ...(result.hints || []),
        `Requested page ${page} is outside available range (1-${totalPages}).`,
        `Use page=${totalPages} for the last available page.`,
      ],
    };
  }

  const startIndex = (page - 1) * referencesPerPage;
  const endIndex = Math.min(startIndex + referencesPerPage, totalReferences);
  const paginatedLocations = result.locations.slice(startIndex, endIndex);

  const hints = [...(result.hints || [])];
  if (page < totalPages) {
    hints.push(
      `Showing page ${page} of ${totalPages}. Use page=${page + 1} for more.`
    );
  }

  return {
    ...result,
    locations: paginatedLocations,
    pagination: {
      currentPage: page,
      totalPages,
      totalResults: totalReferences,
      hasMore: page < totalPages,
      ...(referencesPerPage < Number.MAX_SAFE_INTEGER
        ? { resultsPerPage: referencesPerPage }
        : {}),
    },
    hasMultipleFiles,
    hints,
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

  if (isCompact(query.verbosity)) {
    return {
      ...result,
      hints: compactTrimHints(result.hints, isAdvisoryFindReferencesHint, 2),
    };
  }

  if (!isConcise(query.verbosity)) return result;

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
export { findReferencesWithPatternMatching } from './lspReferencesPatterns.js';
