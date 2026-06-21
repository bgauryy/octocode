import type { GitHubFileContentApiResult } from '../tools/github_fetch_content/types.js';
import { getOutputCharLimit } from '../utils/pagination/charLimit.js';
import { GITHUB_FILE_CONTENT_DEFAULT_CHAR_LENGTH } from '../config.js';
import { ContentSanitizer } from '@octocodeai/octocode-engine/contentSanitizer';
import { contextUtils } from '../utils/contextUtils.js';
import { countLines } from '../utils/core/lines.js';
import { applyPagination } from '../utils/pagination/core.js';
import {
  snapToSemanticBoundary,
  isMidBlockCut,
  findNextBlockBoundary,
} from '../utils/pagination/boundary.js';
import { extractMatchingLines } from '../tools/local_fetch_content/contentExtractor.js';
import { OctokitWithThrottling } from './client.js';
import type { MinifyMode } from '../scheme/fields.js';
import { markdownHeadingOutlineToText } from '../utils/markdownOutline.js';

function getDefaultContentPageSize(): number {
  const globalLimit = getOutputCharLimit();
  return Math.min(globalLimit, GITHUB_FILE_CONTENT_DEFAULT_CHAR_LENGTH);
}

function sourceSizeFields(sourceChars: number, sourceBytes: number) {
  return { sourceChars, sourceBytes };
}

interface FileTimestampInfo {
  lastModified: string;
  lastModifiedBy: string;
}

export function applyContentPagination(
  data: GitHubFileContentApiResult,
  charOffset: number,
  charLength?: number
): GitHubFileContentApiResult {
  const content = data.content ?? '';
  const maxChars = charLength ?? getDefaultContentPageSize();

  if (content.length <= maxChars && charOffset === 0) {
    return data;
  }

  const filePath = data.path ?? undefined;
  const { length: snappedLength, chunkMode } = snapToSemanticBoundary(
    content,
    charOffset,
    maxChars,
    filePath
  );

  const paginationMeta = applyPagination(content, charOffset, snappedLength);

  let nextBlockChar: number | undefined;
  if (paginationMeta.hasMore && chunkMode === 'char-limit') {
    if (isMidBlockCut(paginationMeta.paginatedContent)) {
      const cutPos = paginationMeta.charOffset + paginationMeta.charLength;
      nextBlockChar = findNextBlockBoundary(content, cutPos, filePath);
    }
  }

  return {
    ...data,
    content: paginationMeta.paginatedContent,
    pagination: {
      currentPage: paginationMeta.currentPage,
      totalPages: paginationMeta.totalPages,
      hasMore: paginationMeta.hasMore,
      charOffset: paginationMeta.charOffset,
      charLength: paginationMeta.charLength,
      totalChars: paginationMeta.totalChars,
      chunkMode,
      ...(nextBlockChar !== undefined && { nextBlockChar }),
    },
  };
}

export async function fetchFileTimestamp(
  octokit: InstanceType<typeof OctokitWithThrottling>,
  owner: string,
  repo: string,
  path: string,
  branch?: string
): Promise<FileTimestampInfo | null> {
  try {
    const commits = await octokit.rest.repos.listCommits({
      owner,
      repo,
      path,
      per_page: 1,
      ...(branch && { sha: branch }),
    });

    if (commits.data.length > 0) {
      const lastCommit = commits.data[0];
      const commitDate = lastCommit?.commit?.committer?.date;
      const authorName =
        lastCommit?.commit?.author?.name ||
        lastCommit?.author?.login ||
        'Unknown';

      return {
        lastModified: commitDate || 'Unknown',
        lastModifiedBy: authorName,
      };
    }
    return null;
  } catch {
    return null;
  }
}

export async function processFileContentAPI(
  decodedContent: string,
  owner: string,
  repo: string,
  branch: string,
  filePath: string,
  fullContent: boolean,
  startLine?: number,
  endLine?: number,
  contextLines: number = 5,
  matchString?: string,
  matchStringIsRegex?: boolean,
  matchStringCaseSensitive?: boolean,
  minify: MinifyMode = 'standard'
): Promise<GitHubFileContentApiResult> {
  const sourceChars = decodedContent.length;
  const sourceBytes = Buffer.byteLength(decodedContent, 'utf-8');
  const applyStandardMinify = minify === 'standard' || minify === 'symbols';
  const fallbackContentView = applyStandardMinify ? 'standard' : 'none';

  let signaturesSkippedWarning: string | undefined;
  if (minify === 'symbols') {
    const sigs = contextUtils.extractSignatures(decodedContent, filePath);
    if (sigs === null) {
      const markdownOutline = markdownHeadingOutlineToText(
        decodedContent,
        filePath
      );
      if (markdownOutline !== null) {
        return {
          owner,
          repo,
          path: filePath,
          content: markdownOutline,
          contentView: 'symbols',
          isSkeleton: true,
          branch,
          totalLines: countLines(decodedContent),
          ...sourceSizeFields(sourceChars, sourceBytes),
          isPartial: false,
          signaturesExtracted: true,
          hints: [contextUtils.SIGNATURES_ONLY_HINT],
        };
      }
      signaturesSkippedWarning = `minify:"symbols" is not supported for this file type (${filePath.split('.').pop() ?? 'unknown'}) — falling back to standard content view.`;
    }
    if (sigs !== null) {
      const sanitized = ContentSanitizer.sanitizeContent(sigs, filePath);
      const sigContent = contextUtils.applyContentViewMinification(
        sanitized.content,
        filePath
      );
      const hints: string[] = [contextUtils.SIGNATURES_ONLY_HINT];
      if (matchString) {
        hints.push(
          `matchString was ignored — minify:"symbols" returns the full skeleton index. Use startLine/endLine from the gutter to read the matching body.`
        );
      }
      if (sanitized.hasSecrets) {
        hints.push(
          `Secrets detected and redacted: ${sanitized.secretsDetected.join(', ')}`
        );
      }
      return {
        owner,
        repo,
        path: filePath,
        content: sigContent,
        contentView: 'symbols',
        isSkeleton: true,
        branch,
        totalLines: countLines(decodedContent),
        ...sourceSizeFields(sourceChars, sourceBytes),
        isPartial: false,
        signaturesExtracted: true,
        hints,
      };
    }
  }

  const matchLocationsSet = new Set<string>();

  const originalContent = decodedContent;
  const originalLines = originalContent.split('\n');
  const totalLines = countLines(originalContent);

  let finalContent = decodedContent;
  let actualStartLine: number | undefined;
  let actualEndLine: number | undefined;
  let isPartial = false;
  let matchRanges: Array<{ start: number; end: number }> | undefined;

  if (fullContent) {
    finalContent = decodedContent;
  } else if (matchString) {
    const isCaseSensitive = matchStringCaseSensitive === true;
    let extraction: ReturnType<typeof extractMatchingLines>;
    try {
      extraction = extractMatchingLines(
        originalLines,
        matchString,
        contextLines,
        matchStringIsRegex ?? false,
        isCaseSensitive
      );
    } catch {
      return {
        owner,
        repo,
        path: filePath,
        content: '',
        branch,
        totalLines,
        ...sourceSizeFields(sourceChars, sourceBytes),
        matchNotFound: true,
        searchedFor: matchString,
        hints: [
          `Invalid regex "${matchString}". Check syntax (e.g. escape backslashes: "\\\\w+" not "\\w+") or disable matchStringIsRegex=false for a literal search.`,
        ],
      } as GitHubFileContentApiResult;
    }

    if (extraction.matchCount === 0) {
      const notFoundHints = matchStringIsRegex
        ? [
            `Regex "${matchString}" matched no lines. Verify the pattern, check flags (case-${isCaseSensitive ? 'sensitive' : 'insensitive'}), or use fullContent=true to inspect the file.`,
          ]
        : [
            `"${matchString}" not found in file${isCaseSensitive ? ' (case-sensitive)' : ''}. Try matchStringIsRegex=true for pattern matching, broaden the search, or use fullContent=true.`,
          ];
      return {
        owner,
        repo,
        path: filePath,
        content: '',
        branch,
        totalLines,
        ...sourceSizeFields(sourceChars, sourceBytes),
        matchNotFound: true,
        searchedFor: matchString,
        hints: notFoundHints,
      } as GitHubFileContentApiResult;
    }

    finalContent = extraction.lines.join('\n');
    const firstRange = extraction.matchRanges[0]!;
    const lastRange =
      extraction.matchRanges[extraction.matchRanges.length - 1]!;
    startLine = firstRange.start;
    endLine = lastRange.end;
    actualStartLine = firstRange.start;
    actualEndLine = lastRange.end;
    isPartial = true;
    if (extraction.matchRanges.length > 1) {
      matchRanges = extraction.matchRanges;
    }

    const shownLines = extraction.matchingLines.slice(0, 5).join(', ');
    const extraCount =
      extraction.matchingLines.length > 5
        ? ` and ${extraction.matchingLines.length - 5} more`
        : '';
    matchLocationsSet.add(
      extraction.matchCount > 1
        ? `Found ${extraction.matchCount} occurrences of "${matchString}" on lines ${shownLines}${extraCount} — all shown as ${extraction.matchRanges.length} slice${extraction.matchRanges.length === 1 ? '' : 's'}, ±${contextLines} lines of context each.`
        : `Found "${matchString}" on line ${extraction.matchingLines[0]}`
    );
  } else if (startLine !== undefined || endLine !== undefined) {
    const effectiveStartLine = startLine || 1;

    const effectiveEndLine = endLine || totalLines;

    if (effectiveStartLine < 1 || effectiveStartLine > totalLines) {
      finalContent = decodedContent;
    } else if (effectiveEndLine < effectiveStartLine) {
      finalContent = decodedContent;
    } else {
      const adjustedStartLine = Math.max(1, effectiveStartLine);
      const adjustedEndLine = Math.min(totalLines, effectiveEndLine);

      const selectedLines = originalLines.slice(
        adjustedStartLine - 1,
        adjustedEndLine
      );

      actualStartLine = adjustedStartLine;
      actualEndLine = adjustedEndLine;
      isPartial = true;

      finalContent = selectedLines.join('\n');

      if (effectiveEndLine > totalLines) {
        matchLocationsSet.add(
          `Requested endLine ${effectiveEndLine} adjusted to ${totalLines} (file end)`
        );
      }
    }
  }

  const sanitizationResult = ContentSanitizer.sanitizeContent(
    finalContent,
    filePath
  );
  finalContent = applyStandardMinify
    ? contextUtils.applyContentViewMinification(
        sanitizationResult.content,
        filePath
      )
    : sanitizationResult.content;

  if (sanitizationResult.hasSecrets) {
    matchLocationsSet.add(
      `Secrets detected and redacted: ${sanitizationResult.secretsDetected.join(', ')}`
    );
  }
  if (sanitizationResult.warnings.length > 0) {
    sanitizationResult.warnings.forEach((warning: string) =>
      matchLocationsSet.add(warning)
    );
  }

  if (
    totalLines > 2000 &&
    minify !== 'symbols' &&
    !matchString &&
    !startLine &&
    !endLine &&
    !fullContent
  ) {
    const tailLine = Math.max(1, totalLines - 200);
    matchLocationsSet.add(
      `Large file (${totalLines} lines) — minify:"symbols" for an export index, or startLine=${tailLine} for the tail.`
    );
  }

  const matchLocations = Array.from(matchLocationsSet);

  return {
    owner,
    repo,
    path: filePath,
    content: finalContent,
    ...(fallbackContentView !== 'standard' && {
      contentView: fallbackContentView,
    }),
    branch,
    totalLines,
    ...sourceSizeFields(sourceChars, sourceBytes),
    ...(isPartial && {
      startLine: actualStartLine,
      endLine: actualEndLine,
      isPartial,
    }),
    ...(matchRanges && { matchRanges }),
    ...(matchLocations.length > 0 && {
      matchLocations,
    }),
    ...((matchLocations.length > 0 || signaturesSkippedWarning) && {
      warnings: [
        ...(signaturesSkippedWarning ? [signaturesSkippedWarning] : []),
        ...matchLocations,
      ],
    }),
  } as GitHubFileContentApiResult;
}
