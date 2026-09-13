import {
  paginateContentWindow,
  pageFields,
  fullContentLimit,
} from '../../../utils/file/contentPagination.js';
import { contextUtils } from '../../../utils/contextUtils.js';
import { ContentSanitizer } from '@octocodeai/octocode-engine/contentSanitizer';
import type { LocalFetchToolResult } from '@octocodeai/octocode-core/extra-types';
import type { FetchContentQuery } from '@octocodeai/octocode-core/schema';
import { buildNextPageContinuation } from '../../../scheme/pagination.js';
import { sourceSizeFields, type FileStats } from './validation.js';
import type { ExtractionState } from './types.js';
import { sourcePageRanges } from '../../../utils/file/sourceCitations.js';

export type ContentView = 'none' | 'standard' | 'symbols';

/** Sanitize a complete extracted view before chunk offsets are assigned. */
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
): LocalFetchToolResult {
  const canReadSmallerLineView =
    totalLines > 1 &&
    (query.startLine === undefined || query.startLine !== query.endLine);
  return {
    path: query.path,
    status: 'error',
    errorCode: 'contentSecurityLimit',
    error:
      'The selected content view exceeds the secret scanner size limit. Byte windows cannot safely split unscanned content. Select a smaller source-line range.',
    totalLines,
    isPartial: true,
    terminalLimit: true,
    partialReasons: ['security-selected-view-size-limit'],
    ...(canReadSmallerLineView && {
      next: {
        readBoundedLines: buildNextPageContinuation(
          'localFetch',
          {
            path: query.path,
            startLine: firstSelectedLine,
            endLine: firstSelectedLine,
            minify: 'none',
          },
          'The selected view is too large to scan safely. Read one source line; this starts a different source-line view, not a continuation of the rejected view.'
        ),
      },
    }),
  } as LocalFetchToolResult;
}

export async function buildSuccessResult(
  query: FetchContentQuery,
  extraction: ExtractionState,
  fileStats: FileStats,
  totalLines: number,
  shouldMinify = false,
  contentView: ContentView = shouldMinify ? 'standard' : 'none'
): Promise<LocalFetchToolResult> {
  const source = extraction.resultContent ?? '';
  const outputContent = shouldMinify
    ? contextUtils.applyContentViewMinification(source, String(query.path))
    : source;
  const sanitized = sanitizeReturnedText(outputContent, String(query.path));
  if (sanitized.limited)
    return buildSecurityLimitResult(
      query,
      totalLines,
      extraction.actualStartLine
    );
  const limited = fullContentLimit(
    query,
    sanitized.text,
    totalLines,
    'localFetch'
  );
  if (limited) return limited;
  const window = await paginateContentWindow(
    sanitized.text,
    query,
    'localFetch'
  );
  const warnings = [
    ...(extraction.warnings ?? []),
    ...(sanitized.warning ? [sanitized.warning] : []),
  ];
  const pageSourceLines =
    contentView === 'none'
      ? extraction.sourceLines?.slice(
          window.firstViewLine - 1,
          window.lastViewLine
        )
      : undefined;
  const matchedLines = extraction.matchedLines?.filter(line =>
    pageSourceLines?.includes(line)
  );
  // Minification and redaction can change line placement. Emit a source map only
  // for an unchanged selected view, including disjoint match windows and byte pages.
  const sourceLineRanges =
    contentView === 'none' &&
    sanitized.text === source &&
    (query.matchString === undefined || extraction.sourceLines !== undefined)
      ? sourcePageRanges(window, extraction.sourceLines)
      : [];
  return {
    path: query.path,
    ...pageFields(window),
    contentView,
    ...(sourceLineRanges.length ? { sourceLineRanges } : {}),
    totalLines,
    ...(extraction.actualStartLine !== undefined
      ? {
          startLine: extraction.actualStartLine,
          endLine: extraction.actualEndLine,
        }
      : {}),
    ...(extraction.matchRanges
      ? {
          matchRanges: extraction.matchRanges,
          selectedMatchCount:
            extraction.selectedMatchCount ??
            extraction.matchedLines?.length ??
            0,
          matchedLines: matchedLines ?? [],
        }
      : {}),
    ...(fileStats.mtime ? { modified: fileStats.mtime.toISOString() } : {}),
    ...(warnings.length ? { warnings } : {}),
  };
}

export async function buildSymbolsSkeletonResult(
  query: FetchContentQuery,
  skeleton: string,
  totalLines: number,
  sourceChars: number,
  sourceBytes: number,
  secretWarning: string | undefined
): Promise<LocalFetchToolResult> {
  const limited = fullContentLimit(query, skeleton, totalLines, 'localFetch');
  if (limited)
    return { ...limited, ...sourceSizeFields(sourceChars, sourceBytes) };
  const window = await paginateContentWindow(skeleton, query, 'localFetch');
  return {
    path: query.path,
    ...pageFields(window),
    contentView: 'symbols',
    totalLines,
    ...sourceSizeFields(sourceChars, sourceBytes),
    ...(secretWarning ? { warnings: [secretWarning] } : {}),
  };
}

export function withContentView(
  result: LocalFetchToolResult,
  contentView: ContentView
): LocalFetchToolResult {
  return typeof result.content === 'string'
    ? { ...result, contentView }
    : result;
}
