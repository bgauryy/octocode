import { open, readFile, stat } from 'fs/promises';
import { extractMatchingLines } from './contentExtractor.js';
import { contextUtils } from '../../utils/contextUtils.js';
import { ContentSanitizer } from '@octocodeai/octocode-engine/contentSanitizer';
import {
  applyPagination,
  createPaginationInfo,
} from '../../utils/pagination/core.js';
import {
  snapToSemanticBoundary,
  isMidBlockCut,
  findNextBlockBoundary,
} from '../../utils/pagination/boundary.js';
import { RESOURCE_LIMITS } from '../../utils/core/constants.js';
import { countLines } from '../../utils/core/lines.js';
import { getOutputCharLimit } from '../../utils/pagination/charLimit.js';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import {
  validateToolPath,
  createErrorResult,
} from '../../utils/file/toolHelpers.js';
import type { LocalGetFileContentToolResult } from '@octocodeai/octocode-core/extra-types';
import type { FetchContentQuery } from './scheme.js';
import { ToolErrors } from '../../errors/errorFactories.js';
import { LOCAL_TOOL_ERROR_CODES } from '../../errors/localToolErrors.js';
import { fallbackOnBestEffortFailure } from '../../utils/core/bestEffort.js';
import { attachRawResponseChars } from '../../utils/response/charSavings.js';
import { markdownHeadingOutlineToText } from '../../utils/markdownOutline.js';

type FileStats = NonNullable<Awaited<ReturnType<typeof stat>>>;
type ContentView = 'none' | 'standard' | 'symbols';

interface ExtractionState {
  resultContent?: string;
  isPartial: boolean;
  actualStartLine?: number;
  actualEndLine?: number;
  matchRanges?: Array<{ start: number; end: number }>;
  warnings?: string[];
  earlyResult?: LocalGetFileContentToolResult;
}

function sourceSizeFields(sourceChars: number, sourceBytes: number) {
  const bytesDiff = Math.abs(sourceBytes - sourceChars);
  const significant = bytesDiff >= 50 && bytesDiff / sourceChars >= 0.02;
  return significant ? { sourceChars, sourceBytes } : { sourceChars };
}

function withSourceSize(
  result: LocalGetFileContentToolResult,
  sourceChars: number,
  sourceBytes: number
): LocalGetFileContentToolResult {
  return {
    ...result,
    ...sourceSizeFields(sourceChars, sourceBytes),
  };
}

function validateExtractionOptions(
  query: FetchContentQuery
): LocalGetFileContentToolResult | null {
  const hasFullContent = query.fullContent === true;
  const hasMatchString = query.matchString !== undefined;
  const hasLineRange =
    query.startLine !== undefined || query.endLine !== undefined;

  if (hasFullContent && hasMatchString) {
    const result: LocalGetFileContentToolResult = {
      status: 'error',
      error:
        'Cannot use fullContent with matchString — these are mutually exclusive extraction methods. Choose ONE: fullContent=true to read the entire file, OR matchString to extract matching sections, OR startLine+endLine for a known line range.',
    };
    return result;
  }

  if (hasFullContent && hasLineRange) {
    const result: LocalGetFileContentToolResult = {
      status: 'error',
      error:
        'Cannot use fullContent with startLine/endLine — these are mutually exclusive extraction methods. Choose ONE: fullContent=true to read the entire file, OR startLine+endLine for a known line range, OR matchString to extract matching sections.',
    };
    return result;
  }

  if (hasMatchString && hasLineRange) {
    const result: LocalGetFileContentToolResult = {
      status: 'error',
      error:
        'Cannot use matchString with startLine/endLine — these are mutually exclusive extraction methods. Choose ONE: matchString to extract matching sections, OR startLine+endLine for a known line range, OR fullContent=true to read the entire file.',
    };
    return result;
  }

  const hasStartLine = query.startLine !== undefined;
  const hasEndLine = query.endLine !== undefined;
  if (hasStartLine && !hasEndLine) {
    return {
      status: 'error',
      error: `startLine=${query.startLine} provided without endLine — both are required for line-range extraction.`,
    };
  }
  if (hasEndLine && !hasStartLine) {
    return {
      status: 'error',
      error: `endLine=${query.endLine} provided without startLine — both are required for line-range extraction.`,
    };
  }

  return null;
}

async function getFileStatsOrError(
  query: FetchContentQuery,
  absolutePath: string
): Promise<{
  fileStats?: FileStats;
  errorResult?: LocalGetFileContentToolResult;
}> {
  try {
    return {
      fileStats: await stat(absolutePath),
    };
  } catch (error) {
    const toolError = ToolErrors.fileAccessFailed(
      query.path!,
      error instanceof Error ? error : undefined
    );

    return {
      errorResult: createErrorResult(toolError, query, {
        toolName: TOOL_NAMES.LOCAL_FETCH_CONTENT,
        extra: {
          resolvedPath: absolutePath,
        },
      }) as LocalGetFileContentToolResult,
    };
  }
}

function shouldFailForLargeFile(
  query: FetchContentQuery,
  fileSizeKB: number
): boolean {
  return (
    fileSizeKB > RESOURCE_LIMITS.LARGE_FILE_THRESHOLD_KB &&
    !query.matchString &&
    !query.startLine &&
    !query.fullContent
  );
}

function createLargeFileErrorResult(
  query: FetchContentQuery,
  absolutePath: string,
  fileSizeKB: number
): LocalGetFileContentToolResult {
  const toolError = ToolErrors.fileTooLarge(
    query.path!,
    fileSizeKB,
    RESOURCE_LIMITS.LARGE_FILE_THRESHOLD_KB
  );

  return createErrorResult(toolError, query, {
    toolName: TOOL_NAMES.LOCAL_FETCH_CONTENT,
    extra: { resolvedPath: absolutePath },
  }) as LocalGetFileContentToolResult;
}

function createBinaryFileErrorResult(
  query: FetchContentQuery,
  absolutePath: string
): LocalGetFileContentToolResult {
  const toolError = ToolErrors.binaryFileUnsupported(query.path!);

  return createErrorResult(toolError, query, {
    toolName: TOOL_NAMES.LOCAL_FETCH_CONTENT,
    extra: { resolvedPath: absolutePath },
  }) as LocalGetFileContentToolResult;
}

async function isLikelyBinaryFile(filePath: string): Promise<boolean> {
  const sampleSize = 8192;
  const buffer = Buffer.alloc(sampleSize);
  let handle: Awaited<ReturnType<typeof open>> | undefined;

  try {
    handle = await open(filePath, 'r');
    const { bytesRead } = await handle.read(buffer, 0, sampleSize, 0);
    if (bytesRead === 0) {
      return false;
    }

    const sample = buffer.subarray(0, bytesRead);
    if (sample.includes(0)) {
      return true;
    }

    try {
      new TextDecoder('utf-8', { fatal: true }).decode(sample);
    } catch {
      return true;
    }

    let strippedLength = 0;
    let controlBytes = 0;
    let index = 0;
    while (index < sample.length) {
      const byte = sample[index]!;

      if (
        byte === 0x1b &&
        index + 1 < sample.length &&
        sample[index + 1] === 0x5b
      ) {
        index += 2;
        while (
          index < sample.length &&
          (sample[index]! < 0x40 || sample[index]! > 0x7e)
        ) {
          index += 1;
        }
        index += 1;
        continue;
      }

      strippedLength += 1;
      const allowedControl = byte === 0x09 || byte === 0x0a || byte === 0x0d;
      if (byte < 0x20 && !allowedControl) {
        controlBytes += 1;
      }
      index += 1;
    }

    return strippedLength > 0 && controlBytes / strippedLength > 0.05;
  } catch {
    return false;
  } finally {
    await handle
      ?.close()
      .catch(
        fallbackOnBestEffortFailure('binary sample handle close', undefined)
      );
  }
}

async function readFileContentOrError(
  query: FetchContentQuery,
  absolutePath: string
): Promise<{ content?: string; errorResult?: LocalGetFileContentToolResult }> {
  try {
    return {
      content: await readFile(absolutePath, 'utf-8'),
    };
  } catch (error) {
    const cause = error instanceof Error ? error : undefined;
    const causeCode = (cause as (Error & { code?: string }) | undefined)?.code;
    const toolError =
      causeCode === 'EISDIR'
        ? ToolErrors.fileAccessFailed(query.path!, cause)
        : ToolErrors.fileReadFailed(query.path!, cause);

    return {
      errorResult: createErrorResult(toolError, query, {
        toolName: TOOL_NAMES.LOCAL_FETCH_CONTENT,
        extra: { resolvedPath: absolutePath },
      }) as LocalGetFileContentToolResult,
    };
  }
}

function createNoMatchesResult(
  _query: FetchContentQuery,
  totalLines: number
): LocalGetFileContentToolResult {
  return {
    status: 'empty',
    errorCode: LOCAL_TOOL_ERROR_CODES.NO_MATCHES,
    totalLines,
  };
}

function buildMatchExtractionState(
  query: FetchContentQuery,
  lines: string[],
  totalLines: number
): ExtractionState {
  const result = extractMatchingLines(
    lines,
    query.matchString!,
    (query as { contextLines?: number }).contextLines ?? 5,
    query.matchStringIsRegex ?? false,
    query.matchStringCaseSensitive ?? false
  );

  if (result.lines.length === 0) {
    return {
      isPartial: false,
      earlyResult: createNoMatchesResult(query, totalLines),
    };
  }

  const resultContent = result.lines.join('\n');
  const contextLines = (query as { contextLines?: number }).contextLines ?? 5;
  const shownLines = result.matchingLines.slice(0, 10).join(', ');
  const extraCount =
    result.matchingLines.length > 10
      ? ` (+${result.matchingLines.length - 10} more)`
      : '';
  const matchSummary = `Found ${result.matchCount} occurrence${result.matchCount === 1 ? '' : 's'} of "${query.matchString}" on line${result.matchingLines.length === 1 ? '' : 's'} ${shownLines}${extraCount} — all shown as ${result.matchRanges.length} slice${result.matchRanges.length === 1 ? '' : 's'}, ±${contextLines} lines of context each; these lines are lineHint anchors for lspGetSemantics.`;
  let actualStartLine: number | undefined;
  let actualEndLine: number | undefined;
  let matchRanges: Array<{ start: number; end: number }> | undefined;

  if (result.matchRanges.length > 0) {
    const firstRange = result.matchRanges[0];
    const lastRange = result.matchRanges[result.matchRanges.length - 1];
    if (firstRange && lastRange) {
      actualStartLine = firstRange.start;
      actualEndLine = lastRange.end;
      if (result.matchRanges.length > 1) {
        matchRanges = result.matchRanges;
      }
    }
  }

  return {
    resultContent,
    isPartial: true,
    actualStartLine,
    actualEndLine,
    matchRanges,
    warnings: [matchSummary],
  };
}

function hasLineRangeRequest(query: FetchContentQuery): boolean {
  return query.startLine !== undefined && query.endLine !== undefined;
}

function buildLineRangeExtractionState(
  query: FetchContentQuery,
  lines: string[],
  totalLines: number
): ExtractionState {
  const requestedStartLine = query.startLine!;
  const requestedEndLine = query.endLine!;
  const effectiveStartLine = Math.max(1, requestedStartLine);
  const effectiveEndLine = Math.min(requestedEndLine, totalLines);

  if (requestedEndLine < requestedStartLine) {
    return {
      isPartial: false,
      earlyResult: {
        status: 'empty',
        totalLines,
        errorCode: LOCAL_TOOL_ERROR_CODES.NO_MATCHES,
        warnings: [
          `startLine ${requestedStartLine} is greater than endLine ${requestedEndLine} — startLine must be ≤ endLine`,
          `Use startLine=1 to ${totalLines} with startLine ≤ endLine for a valid range`,
        ],
      },
    };
  }

  if (effectiveStartLine > totalLines) {
    return {
      isPartial: false,
      earlyResult: {
        status: 'empty',
        totalLines,
        errorCode: LOCAL_TOOL_ERROR_CODES.NO_MATCHES,
        warnings: [
          `Requested startLine ${requestedStartLine} exceeds file length (${totalLines} lines)`,
          `Use startLine=1 to ${totalLines} for valid range`,
        ],
      },
    };
  }

  const warnings: string[] = [];
  if (requestedEndLine > totalLines) {
    warnings.push(
      `Requested endLine ${requestedEndLine} adjusted to ${totalLines} (file end)`
    );
  }

  return {
    resultContent: lines
      .slice(effectiveStartLine - 1, effectiveEndLine)
      .join('\n'),
    isPartial: true,
    actualStartLine: effectiveStartLine,
    actualEndLine: effectiveEndLine,
    warnings,
  };
}

function buildExtractionState(
  query: FetchContentQuery,
  content: string,
  _defaultOutputCharLength: number
): ExtractionState {
  const lines = content.split('\n');
  const totalLines = countLines(content);

  if (query.matchString) {
    return buildMatchExtractionState(query, lines, totalLines);
  }

  if (hasLineRangeRequest(query)) {
    return buildLineRangeExtractionState(query, lines, totalLines);
  }

  return {
    resultContent: content,
    isPartial: false,
  };
}

function buildSuccessResult(
  query: FetchContentQuery,
  extraction: ExtractionState,
  fileStats: FileStats,
  totalLines: number,
  defaultOutputCharLength: number,
  shouldMinify = true,
  contentView: ContentView = shouldMinify ? 'standard' : 'none'
): LocalGetFileContentToolResult {
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
  const explicitCharLength = query.charLength;
  const explicitCharOffset = query.charOffset ?? 0;
  let effectiveCharLength: number | undefined = explicitCharLength;
  let autoPaginated = false;
  const charOffset = explicitCharOffset;

  if (
    effectiveCharLength === undefined &&
    !query.fullContent &&
    outputContent.length > defaultOutputCharLength
  ) {
    // fullContent:true is an explicit "give me the WHOLE file in one shot"
    // request — it opts out of the default char-window auto-pagination (the
    // documented contract). Without this guard fullContent was a no-op on files
    // larger than the limit (capped identically to a normal read).
    effectiveCharLength = defaultOutputCharLength;
    autoPaginated = true;
    warnings.push(
      `Auto-paginated: Content (${outputContent.length} chars) exceeds ${defaultOutputCharLength} char limit`
    );
  }

  let chunkMode: 'semantic' | 'char-limit' = 'char-limit';
  let resolvedCharLength = effectiveCharLength;
  if (effectiveCharLength !== undefined) {
    const snap = snapToSemanticBoundary(
      outputContent,
      charOffset,
      effectiveCharLength,
      queryPath
    );
    chunkMode = snap.chunkMode;
    resolvedCharLength = snap.length;
  }

  const pagination = applyPagination(
    outputContent,
    charOffset,
    resolvedCharLength,
    // resolvedCharLength is snapped to a semantic boundary and varies per page;
    // use the stable requested page size for an absolute page counter.
    effectiveCharLength !== undefined
      ? { pageSize: effectiveCharLength }
      : undefined
  );

  const isPartial = extraction.isPartial || pagination.hasMore;

  let nextBlockChar: number | undefined;
  if (
    pagination.hasMore &&
    chunkMode === 'char-limit' &&
    isMidBlockCut(pagination.paginatedContent)
  ) {
    const cutPos = pagination.charOffset + pagination.charLength;
    nextBlockChar = findNextBlockBoundary(outputContent, cutPos, queryPath);
  }

  // Ready continuation query for the next char page. Same shape convention as
  // localSearchCode's `next` map (see ripgrepResultBuilder buildSearchNextMap).
  const next =
    pagination.hasMore && pagination.nextCharOffset !== undefined
      ? {
          continueChars: {
            tool: 'localGetFileContent' as const,
            query: {
              path: queryPath,
              charOffset: pagination.nextCharOffset,
              charLength: effectiveCharLength ?? pagination.charLength,
              minify: query.minify,
            },
          },
        }
      : undefined;

  return {
    path: queryPath,
    content: pagination.paginatedContent,
    ...(contentView !== 'standard' && { contentView }),
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
    ...((effectiveCharLength !== undefined ||
      explicitCharOffset > 0 ||
      autoPaginated) && {
      pagination: {
        ...createPaginationInfo(pagination),
        chunkMode,
        ...(nextBlockChar !== undefined && { nextBlockChar }),
      },
    }),
    ...(next ? { next } : {}),
    ...(warnings.length > 0 && { warnings }),
  };
}

function withContentView(
  result: LocalGetFileContentToolResult,
  contentView: ContentView,
  isSkeleton = false
): LocalGetFileContentToolResult {
  if (typeof result.content !== 'string') return result;
  return {
    ...result,
    contentView,
    ...(isSkeleton ? { isSkeleton: true } : {}),
  };
}

export async function fetchContent(
  query: FetchContentQuery
): Promise<LocalGetFileContentToolResult> {
  const defaultOutputCharLength = getOutputCharLimit();

  try {
    const pathValidation = validateToolPath(
      query,
      TOOL_NAMES.LOCAL_FETCH_CONTENT
    );
    if (!pathValidation.isValid) {
      return pathValidation.errorResult as LocalGetFileContentToolResult;
    }

    const invalidExtractionResult = validateExtractionOptions(query);
    if (invalidExtractionResult) {
      return invalidExtractionResult;
    }

    const absolutePath = pathValidation.sanitizedPath;
    const queryPath = String(query.path);

    const { fileStats, errorResult: fileStatsError } =
      await getFileStatsOrError(query, absolutePath);
    if (fileStatsError || !fileStats) {
      return fileStatsError as LocalGetFileContentToolResult;
    }

    const fileSizeBytes =
      typeof fileStats.size === 'bigint'
        ? Number(fileStats.size)
        : fileStats.size;
    const fileSizeKB = fileSizeBytes / 1024;
    if (await isLikelyBinaryFile(absolutePath)) {
      return attachRawResponseChars(
        createBinaryFileErrorResult(query, absolutePath),
        fileSizeBytes
      );
    }

    if (shouldFailForLargeFile(query, fileSizeKB)) {
      return attachRawResponseChars(
        createLargeFileErrorResult(query, absolutePath, fileSizeKB),
        fileSizeBytes
      );
    }

    const { content: rawContent, errorResult: readError } =
      await readFileContentOrError(query, absolutePath);
    if (readError || rawContent === undefined) {
      return readError as LocalGetFileContentToolResult;
    }

    const sanitized = ContentSanitizer.sanitizeContent(rawContent, queryPath);
    const content = sanitized.content;
    const sourceChars = content.length;
    const sourceBytes = Buffer.byteLength(content, 'utf-8');
    const secretWarning = sanitized.hasSecrets
      ? `Secrets detected and redacted: ${sanitized.secretsDetected.join(', ')}`
      : undefined;

    const minifyMode = query.minify;
    const shouldMinify = minifyMode === 'standard' || minifyMode === 'symbols';
    const fallbackContentView: ContentView = shouldMinify ? 'standard' : 'none';

    let signaturesSkippedWarning: string | undefined;
    if (minifyMode === 'symbols') {
      const sigs = contextUtils.extractSignatures(content, queryPath);
      if (sigs === null) {
        const markdownOutline = markdownHeadingOutlineToText(
          content,
          queryPath
        );
        if (markdownOutline !== null) {
          return attachRawResponseChars(
            {
              path: query.path,
              content: markdownOutline,
              contentView: 'symbols',
              isSkeleton: true,
              totalLines: countLines(content),
              ...sourceSizeFields(sourceChars, sourceBytes),
              ...(secretWarning ? { warnings: [secretWarning] } : {}),
            },
            sourceChars
          );
        }
        signaturesSkippedWarning = `minify:"symbols" is not supported for this file type (${queryPath.split('.').pop() ?? 'unknown'}) — falling back to standard content view.`;
      }
      if (sigs !== null) {
        const totalLinesOrig = countLines(content);
        const sigsProcessed = contextUtils.applyContentViewMinification(
          sigs,
          queryPath
        );

        return attachRawResponseChars(
          {
            path: query.path,
            content: sigsProcessed,
            contentView: 'symbols',
            isSkeleton: true,
            totalLines: totalLinesOrig,
            ...sourceSizeFields(sourceChars, sourceBytes),
            ...(secretWarning ? { warnings: [secretWarning] } : {}),
          },
          sourceChars
        );
      }
    }

    const totalLines = countLines(content);
    const extraction = buildExtractionState(
      query,
      content,
      defaultOutputCharLength
    );

    const withSecretWarning = (
      r: LocalGetFileContentToolResult
    ): LocalGetFileContentToolResult => {
      const appended = [
        ...(signaturesSkippedWarning ? [signaturesSkippedWarning] : []),
        ...(secretWarning ? [secretWarning] : []),
      ];
      if (appended.length === 0) return r;
      const existing = (r as { warnings?: string[] }).warnings ?? [];
      return { ...r, warnings: [...existing, ...appended] };
    };

    if (extraction.earlyResult) {
      const earlyContent = (extraction.earlyResult as { content?: string })
        .content;
      const minifiedEarlyResult =
        shouldMinify && typeof earlyContent === 'string'
          ? {
              ...extraction.earlyResult,
              content: contextUtils.applyContentViewMinification(
                earlyContent,
                queryPath
              ),
            }
          : extraction.earlyResult;
      return attachRawResponseChars(
        withSourceSize(
          withSecretWarning(
            finalizeFetchContentResult(
              withContentView(minifiedEarlyResult, fallbackContentView),
              query,
              totalLines
            )
          ),
          sourceChars,
          sourceBytes
        ),
        sourceChars
      );
    }

    const fullResult = buildSuccessResult(
      query,
      extraction,
      fileStats,
      totalLines,
      defaultOutputCharLength,
      shouldMinify,
      fallbackContentView
    );
    return attachRawResponseChars(
      withSourceSize(
        withSecretWarning(
          finalizeFetchContentResult(fullResult, query, totalLines)
        ),
        sourceChars,
        sourceBytes
      ),
      sourceChars
    );
  } catch (error) {
    return createErrorResult(error, query, {
      toolName: TOOL_NAMES.LOCAL_FETCH_CONTENT,
    }) as LocalGetFileContentToolResult;
  }
}

export function finalizeFetchContentResult(
  result: LocalGetFileContentToolResult,
  _query: FetchContentQuery,
  _totalLines: number
): LocalGetFileContentToolResult {
  return result;
}
