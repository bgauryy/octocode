import { contextUtils } from '../../../utils/contextUtils.js';
import { ContentSanitizer } from '@octocodeai/octocode-engine/contentSanitizer';
import {
  applyPagination,
  createPaginationInfo,
} from '../../../utils/pagination/core.js';
import {
  snapToSemanticBoundary,
  isMidBlockCut,
  findNextBlockBoundary,
} from '../../../utils/pagination/boundary.js';
import type { LocalGetFileContentToolResult } from '@octocodeai/octocode-core/extra-types';
import type { FetchContentQuery } from '../scheme.js';
import {
  buildContinueCharsContinuation,
  buildNextPageContinuation,
} from '../../../scheme/pagination.js';
import { sourceSizeFields, type FileStats } from './validation.js';
import type { ExtractionState } from './extraction.js';

export type ContentView = 'none' | 'standard' | 'symbols';

/** Sanitize a complete extracted view before character offsets are assigned. */
export function sanitizeReturnedText(
  text: string,
  queryPath: string
): { text: string; warning?: string; limited: boolean } {
  const sanitized = ContentSanitizer.sanitizeContent(text, queryPath);
  return {
    text: sanitized.content,
    limited: sanitized.secretsDetected.includes('content-size-exceeded'),
    warning: sanitized.hasSecrets
      ? `Secrets detected and redacted: ${sanitized.secretsDetected.join(', ')}`
      : undefined,
  };
}

export function buildSecurityLimitResult(
  query: FetchContentQuery,
  totalLines: number,
  firstSelectedLine = query.startLine ?? 1
): LocalGetFileContentToolResult {
  const canReadSmallerLineView =
    totalLines > 1 &&
    (query.startLine === undefined || query.startLine !== query.endLine);
  return {
    path: query.path,
    status: 'error',
    errorCode: 'contentSecurityLimit',
    error:
      'The selected content view exceeds the secret scanner size limit. Character windows cannot safely split unscanned content. Select a smaller source-line range.',
    totalLines,
    isPartial: true,
    terminalLimit: true,
    partialReasons: ['security-selected-view-size-limit'],
    ...(canReadSmallerLineView && {
      next: {
        readBoundedLines: buildNextPageContinuation(
          'localGetFileContent',
          {
            path: query.path,
            startLine: firstSelectedLine,
            endLine: firstSelectedLine,
            minify: 'none',
          },
          'The selected view is too large to scan safely. Read one source line; this starts a different source-line view, not a continuation of the rejected character view.'
        ),
      },
    }),
  } as LocalGetFileContentToolResult;
}

export interface ContentWindow {
  /** The windowed (paginated) slice of the input content. */
  windowedContent: string;
  pagination: ReturnType<typeof applyPagination>;
  /** Requested/auto page size (undefined when no windowing applies). */
  effectiveCharLength: number | undefined;
  /** Explicit charOffset requested by the caller (0 when absent). */
  explicitCharOffset: number;
  autoPaginated: boolean;
  /** Warning emitted when the content was auto-paginated (else undefined). */
  autoPaginateWarning?: string;
  chunkMode: 'semantic' | 'char-limit';
  nextBlockChar?: number;
  /** Ready continuation query for the next char page (undefined when !hasMore). */
  next?: {
    continueChars: {
      tool: 'localGetFileContent';
      query: {
        path: string;
        charOffset: number;
        charLength: number;
        minify: FetchContentQuery['minify'];
      };
    };
  };
  /** Whether pagination fields should be emitted on the result. */
  showPagination: boolean;
}

// Pure char-window pagination shared by the normal content path and the
// minify:"symbols" skeleton path. Slices `content` by charOffset/charLength,
// snaps to a semantic boundary, and builds the pagination metadata plus the
// ready `next.continueChars` continuation query. Keeping this in one place lets
// the symbols skeleton window and round-trip exactly like normal content.
export async function paginateContentWindow(
  content: string,
  query: FetchContentQuery,
  defaultOutputCharLength: number
): Promise<ContentWindow> {
  const queryPath = String(query.path);
  const explicitCharLength = query.charLength;
  const explicitCharOffset = query.charOffset ?? 0;
  let effectiveCharLength: number | undefined = explicitCharLength;
  let autoPaginated = false;
  let autoPaginateWarning: string | undefined;
  const charOffset = explicitCharOffset;

  if (
    effectiveCharLength === undefined &&
    !query.fullContent &&
    content.length > defaultOutputCharLength
  ) {
    // fullContent:true is an explicit "give me the WHOLE file in one shot"
    // request — it opts out of the default char-window auto-pagination (the
    // documented contract). Without this guard fullContent was a no-op on files
    // larger than the limit (capped identically to a normal read).
    effectiveCharLength = defaultOutputCharLength;
    autoPaginated = true;
    autoPaginateWarning = `Auto-paginated: Content (${content.length} chars) exceeds ${defaultOutputCharLength} char limit`;
  }

  let chunkMode: 'semantic' | 'char-limit' = 'char-limit';
  let resolvedCharLength = effectiveCharLength;
  if (effectiveCharLength !== undefined) {
    const snap = await snapToSemanticBoundary(
      content,
      charOffset,
      effectiveCharLength,
      queryPath
    );
    chunkMode = snap.chunkMode;
    resolvedCharLength = snap.length;
  }

  const pagination = applyPagination(
    content,
    charOffset,
    resolvedCharLength,
    // resolvedCharLength is snapped to a semantic boundary and varies per page;
    // use the stable requested page size for an absolute page counter.
    effectiveCharLength !== undefined
      ? { pageSize: effectiveCharLength }
      : undefined
  );
  // Semantic boundaries vary in size; offsets are exact, page counts are estimates.
  pagination.pageCountsKind = 'estimated';

  let nextBlockChar: number | undefined;
  if (
    pagination.hasMore &&
    chunkMode === 'char-limit' &&
    isMidBlockCut(pagination.paginatedContent)
  ) {
    const cutPos = pagination.charOffset + pagination.charLength;
    nextBlockChar = await findNextBlockBoundary(content, cutPos, queryPath);
  }

  // Ready continuation query for the next char page. Same shape convention as
  // localSearch's `next` map (see ripgrepResultBuilder buildSearchNextMap).
  // Replay the same extraction before advancing its character window. Keeping
  // the selector matters for line ranges and matchString: a continuation that
  // dropped it would page through the whole source file instead of the content
  // that was actually windowed. Per-call agent metadata is not part of the
  // public query contract and must not leak into a replay.
  const {
    goal: _goal,
    reasoning: _reasoning,
    charOffset: _charOffset,
    ...continuationQuery
  } = query as FetchContentQuery & Record<string, unknown>;
  const next = buildContinueCharsContinuation(
    'localGetFileContent',
    {
      ...continuationQuery,
      path: queryPath,
      charLength: effectiveCharLength ?? pagination.charLength,
    },
    pagination
  ) as ContentWindow['next'];

  return {
    windowedContent: pagination.paginatedContent,
    pagination,
    effectiveCharLength,
    explicitCharOffset,
    autoPaginated,
    autoPaginateWarning,
    chunkMode,
    nextBlockChar,
    next,
    showPagination:
      effectiveCharLength !== undefined ||
      explicitCharOffset > 0 ||
      autoPaginated,
  };
}

export async function buildSuccessResult(
  query: FetchContentQuery,
  extraction: ExtractionState,
  fileStats: FileStats,
  totalLines: number,
  defaultOutputCharLength: number,
  shouldMinify = true,
  contentView: ContentView = shouldMinify ? 'standard' : 'none'
): Promise<LocalGetFileContentToolResult> {
  if (
    !extraction.resultContent ||
    extraction.resultContent.trim().length === 0
  ) {
    return {
      status: 'empty',
      totalLines,
    };
  }

  const warnings = [...(extraction.warnings ?? [])];
  const queryPath = String(query.path);
  const outputContent = shouldMinify
    ? contextUtils.applyContentViewMinification(
        extraction.resultContent,
        queryPath
      )
    : extraction.resultContent;

  // Splitting first can divide a recognizable token into harmless-looking
  // fragments. Continuations must address the same sanitized view on each call.
  const sanitized = sanitizeReturnedText(outputContent, queryPath);
  if (sanitized.limited) {
    return buildSecurityLimitResult(
      query,
      totalLines,
      extraction.actualStartLine
    );
  }
  if (sanitized.warning) warnings.push(sanitized.warning);
  const window = await paginateContentWindow(
    sanitized.text,
    query,
    defaultOutputCharLength
  );
  if (window.autoPaginateWarning) {
    warnings.push(window.autoPaginateWarning);
  }

  const isPartial = extraction.isPartial || window.pagination.hasMore;
  const lineWindowSize =
    extraction.actualStartLine !== undefined &&
    extraction.actualEndLine !== undefined
      ? extraction.actualEndLine - extraction.actualStartLine + 1
      : undefined;
  const continueLines =
    extraction.isPartial &&
    !window.pagination.hasMore &&
    extraction.actualEndLine !== undefined &&
    lineWindowSize !== undefined
      ? buildNextPageContinuation(
          'localGetFileContent',
          {
            path: queryPath,
            startLine: extraction.actualEndLine + 1,
            endLine: Math.min(
              totalLines,
              extraction.actualEndLine + lineWindowSize
            ),
            ...(query.minify !== undefined ? { minify: query.minify } : {}),
          },
          'Continue with the next source-line window.'
        )
      : undefined;
  const next = {
    ...(window.next ?? {}),
    ...(continueLines ? { continueLines } : {}),
  };

  return {
    path: queryPath,
    content: window.windowedContent,
    // Always surface contentView so agents know when default minify:"standard"
    // rewrote the text (previously omitted for standard, which hid the footgun).
    contentView,
    returnedChars: window.windowedContent.length,
    ...(isPartial && { isPartial }),
    totalLines,
    ...(extraction.actualStartLine !== undefined &&
      extraction.actualEndLine !== undefined && {
        startLine: extraction.actualStartLine,
        endLine: extraction.actualEndLine,
        ...(extraction.matchRanges !== undefined && {
          matchRanges: extraction.matchRanges,
        }),
        ...(extraction.matchedLines !== undefined && {
          matchedLines: extraction.matchedLines,
        }),
      }),
    ...(fileStats.mtime && { modified: fileStats.mtime.toISOString() }),
    ...(window.showPagination && {
      pagination: {
        ...createPaginationInfo(window.pagination),
        chunkMode: window.chunkMode,
        ...(window.nextBlockChar !== undefined && {
          nextBlockChar: window.nextBlockChar,
        }),
      },
    }),
    ...(Object.keys(next).length > 0 ? { next } : {}),
    ...(warnings.length > 0 && { warnings }),
  };
}

// Build a minify:"symbols" skeleton result, routing the skeleton text through
// the SAME char-window pagination the normal content path uses. charOffset/
// charLength windows the skeleton, pagination reflects the skeleton's own
// totalChars, and next.continueChars round-trips (query carries minify:"symbols"
// + nextCharOffset). Small skeletons return whole with no pagination/next.
export async function buildSymbolsSkeletonResult(
  query: FetchContentQuery,
  skeleton: string,
  totalLines: number,
  sourceChars: number,
  sourceBytes: number,
  secretWarning: string | undefined,
  defaultOutputCharLength: number
): Promise<LocalGetFileContentToolResult> {
  const window = await paginateContentWindow(
    skeleton,
    query,
    defaultOutputCharLength
  );
  const warnings = [
    ...(window.autoPaginateWarning ? [window.autoPaginateWarning] : []),
    ...(secretWarning ? [secretWarning] : []),
  ];

  return {
    path: query.path,
    content: window.windowedContent,
    contentView: 'symbols',
    returnedChars: window.windowedContent.length,
    ...(window.pagination.hasMore && { isPartial: true }),
    totalLines,
    ...sourceSizeFields(sourceChars, sourceBytes),
    ...(window.showPagination && {
      pagination: {
        ...createPaginationInfo(window.pagination),
        chunkMode: window.chunkMode,
        ...(window.nextBlockChar !== undefined && {
          nextBlockChar: window.nextBlockChar,
        }),
      },
    }),
    ...(window.next ? { next: window.next } : {}),
    ...(warnings.length > 0 && { warnings }),
  };
}

export function withContentView(
  result: LocalGetFileContentToolResult,
  contentView: ContentView
): LocalGetFileContentToolResult {
  if (typeof result.content !== 'string') return result;
  return {
    ...result,
    contentView,
  };
}
