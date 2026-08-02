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
  resolveMinifyMode,
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

/** Redacts secrets from a piece of text that's about to be returned to the
 * caller. Deliberately called on the small, already-extracted/minified
 * output — never on the whole raw file — so its cost scales with what's
 * shipped, not with file size. */
function sanitizeReturnedText(
  text: string,
  queryPath: string
): { text: string; warning?: string } {
  const sanitized = ContentSanitizer.sanitizeContent(text, queryPath);
  return {
    text: sanitized.content,
    warning: sanitized.hasSecrets
      ? `Secrets detected and redacted: ${sanitized.secretsDetected.join(', ')}`
      : undefined,
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

    const minifyModeForGate = resolveMinifyMode(query);
    if (shouldFailForLargeFile(query, fileSizeKB, minifyModeForGate)) {
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

    // sourceChars/sourceBytes always describe the real file, independent of
    // secret redaction — see withSanitizedContent below for why redaction
    // itself is deferred to the content that's actually returned.
    const sourceChars = rawContent.length;
    const sourceBytes = Buffer.byteLength(rawContent, 'utf-8');
    const content = rawContent;

    // Resolve the effective minify mode here rather than via a schema default.
    // `fullContent` promises the whole file verbatim, so it defaults to 'none'
    // (otherwise 'standard' would strip comments/blank lines and "reads the
    // whole file" would be a lie); every other read defaults to 'standard'. An
    // explicit minify always wins. Resolving here (not at the schema) is what
    // lets us tell "caller omitted minify" from "caller chose standard":
    // inputSchema is parsed upstream before execution, applying any schema default.
    // Same resolution the large-file gate above already applied.
    //
    // matchString BLOCKS minification entirely (by design): minify runs AFTER
    // extraction, so a match inside a comment/blank region could be stripped
    // from the very slice whose matchRanges anchor it — evidence contradicting
    // its own anchors. Matched slices are always verbatim; an explicit minify
    // request is answered with a warning, never applied. (symbols+matchString
    // is already rejected by validateExtractionOptions above.)
    const matchStringBlocksMinify =
      query.matchString !== undefined && minifyModeForGate !== 'none';
    const minifyMode = matchStringBlocksMinify ? 'none' : minifyModeForGate;
    const matchStringMinifyWarning =
      matchStringBlocksMinify && query.minify !== undefined
        ? `minify:"${query.minify}" is not applied to matchString extractions — matched slices are returned verbatim so the content always contains the matched text.`
        : undefined;
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
          const sanitized = sanitizeReturnedText(markdownOutline, queryPath);
          return attachRawResponseChars(
            await buildSymbolsSkeletonResult(
              query,
              sanitized.text,
              countLines(content),
              sourceChars,
              sourceBytes,
              sanitized.warning,
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
        const sanitized = sanitizeReturnedText(sigsProcessed, queryPath);

        return attachRawResponseChars(
          await buildSymbolsSkeletonResult(
            query,
            sanitized.text,
            totalLinesOrig,
            sourceChars,
            sourceBytes,
            sanitized.warning,
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

    // Secrets are redacted from whatever content is actually about to be
    // returned (a bounded slice, a signature skeleton, ...) — never from the
    // whole raw file up front. Scanning the whole file regardless of how
    // small the requested window is would be wasteful, and for a file past
    // the scanner's own size cap it used to substitute a single wholesale
    // placeholder for the entire file *before* line-extraction ran, so a
    // bounded startLine/endLine/matchString/charOffset read of a large file
    // silently returned that placeholder instead of the real requested slice
    // (with bogus totalLines/sourceChars to match) — exactly the escape hatch
    // the "file too large" error above tells the caller to use.
    const withSanitizedContent = (
      r: LocalGetFileContentToolResult
    ): LocalGetFileContentToolResult => {
      const text = (r as { content?: unknown }).content;
      if (typeof text !== 'string') return r;
      const sanitized = sanitizeReturnedText(text, queryPath);
      const appended = [
        ...(signaturesSkippedWarning ? [signaturesSkippedWarning] : []),
        ...(matchStringMinifyWarning ? [matchStringMinifyWarning] : []),
        ...(sanitized.warning ? [sanitized.warning] : []),
      ];
      const existing = (r as { warnings?: string[] }).warnings ?? [];
      return {
        ...r,
        content: sanitized.text,
        ...(appended.length > 0 && { warnings: [...existing, ...appended] }),
      };
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
          withSanitizedContent(
            withContentView(minifiedEarlyResult, fallbackContentView)
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
        withSanitizedContent(fullResult),
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
