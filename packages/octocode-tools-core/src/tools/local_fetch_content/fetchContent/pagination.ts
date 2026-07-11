import { contextUtils } from '../../../utils/contextUtils.js';
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
import { buildContinueCharsContinuation } from '../../../scheme/pagination.js';
import { sourceSizeFields, type FileStats } from './validation.js';
import type { ExtractionState } from './extraction.js';

export type ContentView = 'none' | 'standard' | 'symbols';

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
  // localSearchCode's `next` map (see ripgrepResultBuilder buildSearchNextMap).
  const next = buildContinueCharsContinuation(
    'localGetFileContent',
    {
      path: queryPath,
      charLength: effectiveCharLength ?? pagination.charLength,
      minify: query.minify,
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

  const window = await paginateContentWindow(
    outputContent,
    query,
    defaultOutputCharLength
  );
  if (window.autoPaginateWarning) {
    warnings.push(window.autoPaginateWarning);
  }

  const isPartial = extraction.isPartial || window.pagination.hasMore;

  return {
    path: queryPath,
    content: window.windowedContent,
    // Always surface contentView so agents know when default minify:"standard"
    // rewrote the text (previously omitted for standard, which hid the footgun).
    contentView,
    ...(isPartial && { isPartial }),
    totalLines,
    ...(extraction.actualStartLine !== undefined &&
      extraction.actualEndLine !== undefined && {
        startLine: extraction.actualStartLine,
        endLine: extraction.actualEndLine,
        ...(extraction.matchRanges !== undefined && {
          matchRanges: extraction.matchRanges,
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
    ...(window.next ? { next: window.next } : {}),
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
