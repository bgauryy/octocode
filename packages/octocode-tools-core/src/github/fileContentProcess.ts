import type { GitHubFileContentApiResult } from '../tools/github_fetch_content/types.js';
import { ContentSanitizer } from '@octocodeai/octocode-engine/contentSanitizer';
import { contextUtils } from '../utils/contextUtils.js';
import { countLines } from '../utils/core/lines.js';
import { selectMatchingSource } from '../utils/file/contentExtractor.js';
import type { MinifyMode } from '@octocodeai/octocode-core/schema';
import { markdownHeadingOutlineToText } from '../utils/markdownOutline.js';

export async function processFileContentAPI(
  decodedContent: string,
  owner: string,
  repo: string,
  branch: string,
  filePath: string,
  fullContent: boolean,
  startLine?: number,
  endLine?: number,
  contextLines = 5,
  matchString?: string,
  matchStringIsRegex?: boolean,
  matchStringCaseSensitive?: boolean,
  minify: MinifyMode = 'none',
  contextBytes?: number
): Promise<GitHubFileContentApiResult> {
  const totalLines = countLines(decodedContent);
  const base = {
    owner,
    repo,
    path: filePath,
    branch,
    totalLines,
    sourceChars: decodedContent.length,
    sourceBytes: Buffer.byteLength(decodedContent),
  };
  let content = decodedContent;
  let contentView: MinifyMode = matchString !== undefined ? 'none' : minify;
  let minifyFallback: GitHubFileContentApiResult['minifyFallback'];
  let selection: Partial<GitHubFileContentApiResult> = {};
  if (matchString !== undefined && !fullContent) {
    const selected = selectMatchingSource(
      decodedContent,
      matchString,
      contextLines,
      matchStringIsRegex ?? false,
      matchStringCaseSensitive ?? false,
      contextBytes,
      filePath
    );
    if (selected.securityLimited)
      return {
        ...base,
        content: '',
        errorCode: 'contentSecurityLimit',
        isPartial: true,
        terminalLimit: true,
        partialReasons: ['security-selected-view-size-limit'],
      };
    content = selected.content;
    selection = {
      startLine: selected.matchRanges[0]?.start,
      endLine: selected.matchRanges.at(-1)?.end,
      matchRanges: selected.matchRanges,
      matchedLines: selected.matchingLines,
      sourceLines: selected.sourceLines,
      ...(selected.warnings.length ? { warnings: selected.warnings } : {}),
      selectedMatchCount: selected.matchCount,
      ...(selected.matchCount === 0
        ? { matchNotFound: true, searchedFor: matchString }
        : {}),
    };
    if (minify !== 'none')
      minifyFallback = {
        requested: minify,
        applied: 'none',
        reason: 'match-evidence',
      };
  } else if (!fullContent && startLine !== undefined && endLine !== undefined) {
    const records = decodedContent.match(/[^\n]*\n|[^\n]+$/g) ?? [];
    if (startLine > totalLines || endLine < startLine)
      return {
        ...base,
        content: '',
        errorCode: 'noMatches',
        contentView: 'none',
      };
    const end = Math.min(endLine, totalLines);
    content = records
      .slice(startLine - 1, Math.max(startLine - 1, end))
      .join('');
    selection = { startLine, endLine: end };
  }
  if (contentView === 'symbols') {
    const signatures = contextUtils.extractSignatures(content, filePath);
    const outline =
      signatures === null
        ? markdownHeadingOutlineToText(content, filePath)
        : null;
    if (signatures !== null) {
      content = contextUtils.applyContentViewMinification(signatures, filePath);
    } else if (outline !== null) {
      content = outline;
    } else {
      contentView = 'standard';
      minifyFallback = {
        requested: 'symbols',
        applied: 'standard',
        reason: 'outline-unavailable',
      };
    }
  }
  if (contentView === 'standard')
    content = contextUtils.applyContentViewMinification(content, filePath);
  const sanitized = ContentSanitizer.sanitizeContent(content, filePath);
  if (sanitized.secretsDetected.includes('content-size-exceeded')) {
    return {
      ...base,
      content: '',
      errorCode: 'contentSecurityLimit',
      isPartial: true,
      terminalLimit: true,
      partialReasons: ['security-selected-view-size-limit'],
    };
  }
  const { sourceLines, ...selectionFields } = selection;
  // Only unchanged exact views retain source coordinates. Byte selection may
  // already have discarded its map after redaction or synthetic separators;
  // never substitute an identity map for that missing selected-view evidence.
  const exactSource = contentView === 'none' && sanitized.content === content;
  return {
    ...base,
    ...selectionFields,
    ...(exactSource
      ? matchString !== undefined && !fullContent
        ? sourceLines !== undefined
          ? { sourceLines }
          : {}
        : { sourceLineOffset: (selection.startLine ?? 1) - 1 }
      : {}),
    content: sanitized.content,
    contentView,
    ...(minifyFallback ? { minifyFallback } : {}),
    ...(sanitized.hasSecrets
      ? {
          warnings: [
            `Secrets detected and redacted: ${sanitized.secretsDetected.join(', ')}`,
          ],
        }
      : {}),
  };
}
