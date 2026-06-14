import type { GitHubFileContentApiResult } from '../tools/github_fetch_content/types.js';
import { getOutputCharLimit } from '../utils/pagination/charLimit.js';
import { GITHUB_FILE_CONTENT_DEFAULT_CHAR_LENGTH } from '../config.js';
import { ContentSanitizer } from 'octocode-security/contentSanitizer';
import {
  applyContentViewMinification,
  extractSignatures,
  SIGNATURES_ONLY_HINT,
} from '@octocodeai/octocode-minifier-utils';
import { applyPagination } from '../utils/pagination/core.js';
import {
  isMidBlockCut,
  findNextBlockBoundary,
} from '../utils/pagination/boundary.js';
import { extractMatchingLines } from '../tools/local_fetch_content/contentExtractor.js';
import { OctokitWithThrottling } from './client.js';
import type { MinifyMode } from '../scheme/fields.js';

function getDefaultContentPageSize(): number {
  // Use the global output limit only if it was explicitly lowered below the
  // tool-specific default; otherwise use the tool-specific 1000-char budget.
  // This keeps pages tight (cache re-use, low token cost) while still
  // respecting operator overrides via OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH.
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

  const paginationMeta = applyPagination(content, charOffset, maxChars);

  // Detect mid-block cuts: if the page ends inside an indented block (the last
  // non-empty line has leading whitespace), find the next top-level semantic
  // boundary so the finalizer can emit a targeted "extend charLength" hint.
  // Language is derived from data.path (e.g. "src/react.ts" → ext "ts").
  let nextBlockChar: number | undefined;
  if (paginationMeta.hasMore) {
    if (isMidBlockCut(paginationMeta.paginatedContent)) {
      const cutPos = paginationMeta.charOffset + paginationMeta.charLength;
      nextBlockChar = findNextBlockBoundary(content, cutPos, data.path ?? undefined);
    }
  }

  // Tool-level pagination hints are emitted by the github_fetch_content
  // finalizer (buildRuntimeHints) from `pagination`; the provider boundary
  // (transformFileContentResult) does not carry per-file `hints`, so none are
  // attached here.
  //
  // Byte fields are intentionally omitted — pagination is char-based.
  // Consumers must use charOffset (not byteOffset) as the continuation cursor.
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
  // "symbols" implies the standard comment/whitespace strip on whatever
  // content leaves this function (skeleton, or full-content fallback).
  const applyStandardMinify = minify === 'standard' || minify === 'symbols';
  const fallbackContentView = applyStandardMinify ? 'standard' : 'none';

  let signaturesSkippedWarning: string | undefined;
  if (minify === 'symbols') {
    const sigs = extractSignatures(decodedContent, filePath);
    if (sigs === null) {
      signaturesSkippedWarning = `minify:"symbols" is not supported for this file type (${filePath.split('.').pop() ?? 'unknown'}) — falling back to standard content view.`;
    }
    if (sigs !== null) {
      // Redact secrets in the skeleton too (a top-level `const KEY = "…"`
      // matches a signature pattern) — same ContentSanitizer pass the normal
      // content path runs below, keeping local and GitHub aligned.
      const sanitized = ContentSanitizer.sanitizeContent(sigs, filePath);
      const sigContent = applyContentViewMinification(
        sanitized.content,
        filePath
      );
      const hints: string[] = [SIGNATURES_ONLY_HINT];
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
        totalLines: decodedContent.split('\n').length,
        ...sourceSizeFields(sourceChars, sourceBytes),
        // Skeletons bypass applyContentPagination — returned whole. isSkeleton
        // carries the lossy "bodies omitted" signal, while isPartial remains
        // false so agents do not try to paginate a complete skeleton index.
        isPartial: false,
        signaturesExtracted: true,
        hints,
      };
    }
  }

  const matchLocationsSet = new Set<string>();

  const originalContent = decodedContent;
  const originalLines = originalContent.split('\n');
  const totalLines = originalLines.length;

  let finalContent = decodedContent;
  let actualStartLine: number | undefined;
  let actualEndLine: number | undefined;
  let isPartial = false;
  let matchRanges: Array<{ start: number; end: number }> | undefined;

  if (fullContent) {
    finalContent = decodedContent;
  } else if (matchString) {
    // Same multi-occurrence extraction as localGetFileContent: ALL matches are
    // returned as merged context slices (with "[N lines omitted]" separators),
    // not just the first hit. Oversized results are char-paginated downstream.
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
    ? applyContentViewMinification(sanitizationResult.content, filePath)
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

  // Large-file navigation: when no narrowing was requested and the file is big,
  // guide the agent to use startLine for tail access and minify:"symbols" for
  // an export index — avoids agents giving up on large files they can read.
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
    // Omit contentView when 'standard' (default) — absence implies standard.
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
