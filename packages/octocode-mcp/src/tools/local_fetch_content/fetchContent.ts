import { open, readFile, stat } from 'fs/promises';
import { getHints } from '../../hints/index.js';
import { extractMatchingLines } from './contentExtractor.js';
import {
  applyContentViewMinification,
  extractSignatures,
  SIGNATURES_ONLY_HINT,
} from '../../utils/minifier/applyMinification.js';
import { ContentSanitizer } from 'octocode-security-utils/contentSanitizer';
import {
  applyPagination,
  createPaginationInfo,
} from '../../utils/pagination/core.js';
import { generatePaginationHints } from '../../utils/pagination/hints.js';
import { RESOURCE_LIMITS } from '../../utils/core/constants.js';
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

type FileStats = Awaited<ReturnType<typeof stat>>;

interface ExtractionState {
  resultContent?: string;
  isPartial: boolean;
  actualStartLine?: number;
  actualEndLine?: number;
  matchRanges?: Array<{ start: number; end: number }>;
  warnings?: string[];
  earlyResult?: LocalGetFileContentToolResult;
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
      hints: [
        'fullContent and matchString are mutually exclusive. Pick one — matchString is more token-efficient when you know what to look for.',
      ],
    };
    return result;
  }

  if (hasFullContent && hasLineRange) {
    const result: LocalGetFileContentToolResult = {
      status: 'error',
      error:
        'Cannot use fullContent with startLine/endLine — these are mutually exclusive extraction methods. Choose ONE: fullContent=true to read the entire file, OR startLine+endLine for a known line range, OR matchString to extract matching sections.',
      hints: [
        'fullContent and startLine/endLine are mutually exclusive. Pick one extraction mode so line ranges are never silently ignored.',
      ],
    };
    return result;
  }

  if (hasMatchString && hasLineRange) {
    const result: LocalGetFileContentToolResult = {
      status: 'error',
      error:
        'Cannot use matchString with startLine/endLine — these are mutually exclusive extraction methods. Choose ONE: matchString to extract matching sections, OR startLine+endLine for a known line range, OR fullContent=true to read the entire file.',
      hints: [
        'matchString and startLine/endLine are mutually exclusive. Use matchString for search-driven extraction or startLine/endLine for a known range.',
      ],
    };
    return result;
  }

  const hasStartLine = query.startLine !== undefined;
  const hasEndLine = query.endLine !== undefined;
  if (hasStartLine && !hasEndLine) {
    return {
      status: 'error',
      error: `startLine=${query.startLine} provided without endLine — both are required for line-range extraction.`,
      hints: [
        `Add endLine to complete the range, e.g. endLine=${query.startLine! + 50}.`,
        'Use matchString for search-driven extraction when you do not know the exact end line.',
      ],
    };
  }
  if (hasEndLine && !hasStartLine) {
    return {
      status: 'error',
      error: `endLine=${query.endLine} provided without startLine — both are required for line-range extraction.`,
      hints: [
        `Add startLine to complete the range, e.g. startLine=1.`,
        'Use matchString for search-driven extraction when you do not know the exact start line.',
      ],
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
    // query.path is non-null: validated by validateToolPath before this runs.
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
        hintContext: { path: query.path },
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
  // query.path is non-null: validated by validateToolPath before this runs.
  const toolError = ToolErrors.fileTooLarge(
    query.path!,
    fileSizeKB,
    RESOURCE_LIMITS.LARGE_FILE_THRESHOLD_KB
  );

  return createErrorResult(toolError, query, {
    toolName: TOOL_NAMES.LOCAL_FETCH_CONTENT,
    extra: { resolvedPath: absolutePath },
    hintContext: { fileSize: fileSizeKB * 1024, isLarge: true },
  }) as LocalGetFileContentToolResult;
}

function createBinaryFileErrorResult(
  query: FetchContentQuery,
  absolutePath: string
): LocalGetFileContentToolResult {
  // query.path is non-null: validated by validateToolPath before this runs.
  const toolError = ToolErrors.binaryFileUnsupported(query.path!);

  return createErrorResult(toolError, query, {
    toolName: TOOL_NAMES.LOCAL_FETCH_CONTENT,
    extra: { resolvedPath: absolutePath },
    customHints: ['Binary or non-UTF-8 content.'],
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
      // index < sample.length is guaranteed by the loop condition above.
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
    // query.path is non-null: validated by validateToolPath before this runs.
    const toolError = ToolErrors.fileReadFailed(
      query.path!,
      error instanceof Error ? error : undefined
    );

    return {
      errorResult: createErrorResult(toolError, query, {
        toolName: TOOL_NAMES.LOCAL_FETCH_CONTENT,
        extra: { resolvedPath: absolutePath },
      }) as LocalGetFileContentToolResult,
    };
  }
}

function createNoMatchesResult(
  query: FetchContentQuery,
  totalLines: number
): LocalGetFileContentToolResult {
  const hints: string[] = [
    `No matches for "${query.matchString}" in ${query.path} (${totalLines} line${totalLines === 1 ? '' : 's'} scanned).`,
  ];
  if (query.matchStringIsRegex) {
    hints.push('Regex is per-line only — verify the pattern fits on one line.');
  } else {
    hints.push(
      'Try matchStringIsRegex=true for pattern matching (e.g. "export.*function").'
    );
  }
  if (query.matchStringCaseSensitive) {
    hints.push('caseSensitive=true is active — disable for fuzzier matching.');
  }
  return {
    status: 'empty',
    errorCode: LOCAL_TOOL_ERROR_CODES.NO_MATCHES,
    totalLines,
    hints,
  };
}

function buildMatchExtractionState(
  query: FetchContentQuery,
  lines: string[],
  totalLines: number,
  defaultOutputCharLength: number
): ExtractionState {
  const result = extractMatchingLines(
    lines,
    // matchString is present: this builder runs only on the matchString branch.
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
  let actualStartLine: number | undefined;
  let actualEndLine: number | undefined;
  let matchRanges: Array<{ start: number; end: number }> | undefined;

  if (result.matchRanges.length > 0) {
    const firstRange = result.matchRanges[0];
    const lastRange = result.matchRanges[result.matchRanges.length - 1];
    if (firstRange && lastRange) {
      actualStartLine = firstRange.start;
      actualEndLine = lastRange.end;
      matchRanges = result.matchRanges;
    }
  }

  if (resultContent.length > defaultOutputCharLength) {
    // Content is char-paginated via charOffset (page is the list cursor).
    const charOffset = query.charOffset ?? 0;
    const autoPagination = applyPagination(
      resultContent,
      charOffset,
      defaultOutputCharLength
    );
    return {
      isPartial: true,
      earlyResult: {
        content: autoPagination.paginatedContent,
        isPartial: true,
        totalLines,
        ...(actualStartLine !== undefined && {
          startLine: actualStartLine,
          endLine: actualEndLine,
          matchRanges,
        }),
        pagination: createPaginationInfo(autoPagination),
        warnings: [
          `Auto-paginated: ${result.matchCount} matches exceeded display limit`,
          ...(matchRanges && matchRanges.length > 0
            ? [
                'matchRanges covers this chunk only — use the charOffset cursor to read further match positions.',
              ]
            : []),
        ],
        hints: generatePaginationHints(autoPagination, {
          toolName: TOOL_NAMES.LOCAL_FETCH_CONTENT,
        }),
      },
    };
  }

  return {
    resultContent,
    isPartial: true,
    actualStartLine,
    actualEndLine,
    matchRanges,
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
  // Both are defined here — this builder runs only when hasLineRangeRequest() is true.
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
        hints: [
          ...getHints(TOOL_NAMES.LOCAL_FETCH_CONTENT, 'empty'),
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
        hints: [
          ...getHints(TOOL_NAMES.LOCAL_FETCH_CONTENT, 'empty'),
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
  defaultOutputCharLength: number
): ExtractionState {
  const lines = content.split('\n');
  const totalLines = lines.length;

  if (query.matchString) {
    return buildMatchExtractionState(
      query,
      lines,
      totalLines,
      defaultOutputCharLength
    );
  }

  if (hasLineRangeRequest(query)) {
    return buildLineRangeExtractionState(query, lines, totalLines);
  }

  return {
    resultContent: content,
    isPartial: false,
  };
}

function lineRangeContinuationHints(r: {
  isPartial?: boolean;
  startLine?: number;
  endLine?: number;
  totalLines?: number;
  matchRanges?: unknown;
}): string[] {
  if (
    r.isPartial === true &&
    r.matchRanges === undefined &&
    typeof r.startLine === 'number' &&
    typeof r.endLine === 'number' &&
    typeof r.totalLines === 'number' &&
    r.endLine < r.totalLines
  ) {
    const remaining = r.totalLines - r.endLine;
    return [
      `More content: use startLine=${r.endLine + 1} to continue (${remaining} line${remaining === 1 ? '' : 's'} remaining)`,
    ];
  }
  return [];
}

function buildSuccessResult(
  query: FetchContentQuery,
  extraction: ExtractionState,
  fileStats: FileStats,
  totalLines: number,
  defaultOutputCharLength: number,
  shouldMinify = true
): LocalGetFileContentToolResult {
  if (
    !extraction.resultContent ||
    extraction.resultContent.trim().length === 0
  ) {
    return {
      status: 'empty',
      totalLines,
      hints: getHints(TOOL_NAMES.LOCAL_FETCH_CONTENT, 'empty'),
    };
  }

  const warnings = [...(extraction.warnings ?? [])];
  const explicitCharLength = query.charLength;
  const explicitCharOffset = query.charOffset ?? 0;
  let effectiveCharLength: number | undefined = explicitCharLength;
  let autoPaginated = false;
  const charOffset = explicitCharOffset;

  if (
    effectiveCharLength === undefined &&
    extraction.resultContent.length > defaultOutputCharLength
  ) {
    effectiveCharLength = defaultOutputCharLength;
    autoPaginated = true;
    // charOffset already holds the explicit content cursor (query.charOffset).
    warnings.push(
      `Auto-paginated: Content (${extraction.resultContent.length} chars) exceeds ${defaultOutputCharLength} char limit`
    );
  }

  const pagination = applyPagination(
    extraction.resultContent,
    charOffset,
    effectiveCharLength
  );

  const isPartial = extraction.isPartial || pagination.hasMore;

  const baseHints: string[] = lineRangeContinuationHints({
    isPartial,
    startLine: extraction.actualStartLine,
    endLine: extraction.actualEndLine,
    totalLines,
    matchRanges: extraction.matchRanges,
  });

  const paginationHints =
    effectiveCharLength || autoPaginated
      ? generatePaginationHints(pagination, {
          toolName: TOOL_NAMES.LOCAL_FETCH_CONTENT,
        })
      : [];

  // Large-file navigation hints: when the file is large and no narrowing was
  // requested, guide agents to use startLine for tail access and signaturesOnly
  // for an export index — prevents agents giving up on navigable large files.
  const largeFileHints: string[] = [];
  const querySignaturesOnly = query.signaturesOnly;
  if (
    totalLines > 2000 &&
    !querySignaturesOnly &&
    !query.matchString &&
    !query.startLine &&
    !query.endLine &&
    !query.fullContent &&
    pagination.hasMore
  ) {
    const tailLine = Math.max(1, totalLines - 200);
    largeFileHints.push(
      `Large file (${totalLines} lines) — signaturesOnly=true for an export index, or startLine=${tailLine} for the tail.`
    );
  }

  const queryPath = String(query.path);

  return {
    path: queryPath,
    content: shouldMinify
      ? applyContentViewMinification(pagination.paginatedContent, queryPath)
      : pagination.paginatedContent,
    isPartial,
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
      pagination: createPaginationInfo(pagination),
    }),
    ...(warnings.length > 0 && { warnings }),
    hints: [...baseHints, ...paginationHints, ...largeFileHints],
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

    // Redact secrets once at read so every downstream output (signatures,
    // match/range slices, full content) is sanitized — mirrors the GitHub
    // path's ContentSanitizer pass.
    const sanitized = ContentSanitizer.sanitizeContent(rawContent, queryPath);
    const content = sanitized.content;
    const secretWarning = sanitized.hasSecrets
      ? `Secrets detected and redacted: ${sanitized.secretsDetected.join(', ')}`
      : undefined;

    const shouldMinify = query.minify !== false;

    if (query.signaturesOnly) {
      const sigs = extractSignatures(content, queryPath);
      if (sigs !== null) {
        const totalLinesOrig = content.split('\n').length;
        const sigsProcessed = shouldMinify
          ? applyContentViewMinification(sigs, queryPath)
          : sigs;

        // Char-paginate the skeleton at the output budget, mirroring the
        // GitHub path (applyContentPagination) so a large export index stays
        // navigable instead of overflowing the response.
        if (sigsProcessed.length > defaultOutputCharLength) {
          const charOffset = query.charOffset ?? 0;
          const sigPagination = applyPagination(
            sigsProcessed,
            charOffset,
            defaultOutputCharLength
          );
          return attachRawResponseChars(
            {
              path: query.path,
              content: sigPagination.paginatedContent,
              isPartial: true,
              totalLines: totalLinesOrig,
              pagination: createPaginationInfo(sigPagination),
              hints: [
                SIGNATURES_ONLY_HINT,
                ...generatePaginationHints(sigPagination, {
                  toolName: TOOL_NAMES.LOCAL_FETCH_CONTENT,
                }),
                ...(secretWarning ? [secretWarning] : []),
              ],
            },
            content.length
          );
        }

        return attachRawResponseChars(
          {
            path: query.path,
            content: sigsProcessed,
            isPartial: true,
            totalLines: totalLinesOrig,
            hints: [
              SIGNATURES_ONLY_HINT,
              ...(secretWarning ? [secretWarning] : []),
            ],
          },
          content.length
        );
      }
    }

    const totalLines = content.split('\n').length;
    const extraction = buildExtractionState(
      query,
      content,
      defaultOutputCharLength
    );

    // Surface the redaction warning alongside whatever warnings each path
    // already produced (content is already sanitized above).
    const withSecretWarning = (
      r: LocalGetFileContentToolResult
    ): LocalGetFileContentToolResult => {
      if (!secretWarning) return r;
      const existing = (r as { warnings?: string[] }).warnings ?? [];
      return { ...r, warnings: [...existing, secretWarning] };
    };

    if (extraction.earlyResult) {
      // Apply minification to the overflow match-pagination earlyResult (has
      // a content string) — no-matches and line-range error earlyResults have
      // no content so the typeof guard makes this a no-op for those cases.
      const earlyContent = (extraction.earlyResult as { content?: string })
        .content;
      const minifiedEarlyResult =
        shouldMinify && typeof earlyContent === 'string'
          ? {
              ...extraction.earlyResult,
              content: applyContentViewMinification(earlyContent, queryPath),
            }
          : extraction.earlyResult;
      return attachRawResponseChars(
        withSecretWarning(
          finalizeFetchContentResult(minifiedEarlyResult, query, totalLines)
        ),
        content.length
      );
    }

    const fullResult = buildSuccessResult(
      query,
      extraction,
      fileStats,
      totalLines,
      defaultOutputCharLength,
      shouldMinify
    );
    return attachRawResponseChars(
      withSecretWarning(
        finalizeFetchContentResult(fullResult, query, totalLines)
      ),
      content.length
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
