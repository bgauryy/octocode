import { ContentSanitizer } from '@octocodeai/octocode-engine/contentSanitizer';
import { contextUtils } from '../../utils/contextUtils.js';
import { countLines } from '../../utils/core/lines.js';
import { getOutputCharLimit } from '../../utils/pagination/charLimit.js';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import {
  validateToolPath,
  createErrorResult,
} from '../../utils/file/toolHelpers.js';
import type { LocalGetFileContentToolResult } from '@octocodeai/octocode-core/extra-types';
import type { FetchContentQuery } from './scheme.js';
import { attachRawResponseChars } from '../../utils/response/charSavings.js';
import { markdownHeadingOutlineToText } from '../../utils/markdownOutline.js';
import {
  validateExtractionOptions,
  getFileStatsOrError,
  shouldFailForLargeFile,
  createLargeFileErrorResult,
  createBinaryFileErrorResult,
  isLikelyBinaryFile,
  readFileContentOrError,
  withSourceSize,
} from './fetchContent/validation.js';
import { buildExtractionState } from './fetchContent/extraction.js';
import {
  buildSuccessResult,
  buildSymbolsSkeletonResult,
  withContentView,
  type ContentView,
} from './fetchContent/pagination.js';

// Re-exported so existing external imports of these symbols from
// `fetchContent.js` (e.g. `./fetchContent/validation.js`'s FileStats,
// `./fetchContent/pagination.js`'s ContentView) keep resolving unchanged.
export type { ContentView };

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
            await buildSymbolsSkeletonResult(
              query,
              markdownOutline,
              countLines(content),
              sourceChars,
              sourceBytes,
              secretWarning,
              defaultOutputCharLength
            ),
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
          await buildSymbolsSkeletonResult(
            query,
            sigsProcessed,
            totalLinesOrig,
            sourceChars,
            sourceBytes,
            secretWarning,
            defaultOutputCharLength
          ),
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

    const fullResult = await buildSuccessResult(
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
