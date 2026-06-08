import type { GitHubFileContentApiResult } from '../tools/github_fetch_content/types.js';
import { getOutputCharLimit } from '../utils/pagination/charLimit.js';
import { ContentSanitizer } from 'octocode-security-utils/contentSanitizer';
import {
  applyContentViewMinification,
  extractSignatures,
  SIGNATURES_ONLY_HINT,
} from '../utils/minifier/applyMinification.js';
import {
  applyPagination,
  createPaginationInfo,
} from '../utils/pagination/core.js';
import { OctokitWithThrottling } from './client.js';

function getDefaultContentPageSize(): number {
  return getOutputCharLimit();
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

  const totalBytes = Buffer.byteLength(content, 'utf-8');
  if (totalBytes <= maxChars && charOffset === 0) {
    return data;
  }

  const paginationMeta = applyPagination(content, charOffset, maxChars, {
    mode: 'bytes',
  });
  const paginationInfo = createPaginationInfo(paginationMeta);

  // Tool-level pagination hints are emitted by the github_fetch_content
  // finalizer (buildRuntimeHints) from `pagination`; the provider boundary
  // (transformFileContentResult) does not carry per-file `hints`, so none are
  // attached here.
  return {
    ...data,
    content: paginationMeta.paginatedContent,
    pagination: paginationInfo,
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
  matchStringContextLines: number = 5,
  matchString?: string,
  signaturesOnly?: boolean
): Promise<GitHubFileContentApiResult> {
  if (signaturesOnly) {
    const sigs = extractSignatures(decodedContent, filePath);
    if (sigs !== null) {
      // Redact secrets in the skeleton too (a top-level `const KEY = "…"`
      // matches a signature pattern) — same ContentSanitizer pass the normal
      // content path runs below, keeping local and GitHub aligned.
      const sanitized = ContentSanitizer.sanitizeContent(sigs, filePath);
      return {
        owner,
        repo,
        path: filePath,
        content: sanitized.content,
        branch,
        totalLines: decodedContent.split('\n').length,
        isPartial: true,
        hints: sanitized.hasSecrets
          ? [
              SIGNATURES_ONLY_HINT,
              `Secrets detected and redacted: ${sanitized.secretsDetected.join(', ')}`,
            ]
          : [SIGNATURES_ONLY_HINT],
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

  if (fullContent) {
    finalContent = decodedContent;
  } else if (matchString) {
    const matchingLines: number[] = [];

    const searchLower = matchString.toLowerCase();
    for (let i = 0; i < originalLines.length; i++) {
      if (originalLines[i]?.toLowerCase().includes(searchLower)) {
        matchingLines.push(i + 1);
      }
    }

    if (matchingLines.length === 0) {
      const needle = searchLower.replace(/\s+/g, '');
      if (needle.length > 0) {
        for (let i = 0; i < originalLines.length; i++) {
          const haystack = (originalLines[i] ?? '')
            .toLowerCase()
            .replace(/\s+/g, '');
          if (haystack.includes(needle)) {
            matchingLines.push(i + 1);
          }
        }
      }
    }

    if (matchingLines.length === 0) {
      return {
        owner,
        repo,
        path: filePath,
        content: '',
        branch,
        totalLines,
        matchNotFound: true,
        searchedFor: matchString,
        hints: [
          `Pattern "${matchString}" not found in file. Try broader search or verify path.`,
        ],
      } as GitHubFileContentApiResult;
    }

    const firstMatch = matchingLines[0]!;
    const matchStartLine = Math.max(1, firstMatch - matchStringContextLines);
    const matchEndLine = Math.min(
      totalLines,
      firstMatch + matchStringContextLines
    );

    startLine = matchStartLine;
    endLine = matchEndLine;

    const selectedLines = originalLines.slice(matchStartLine - 1, matchEndLine);
    finalContent = selectedLines.join('\n');

    actualStartLine = matchStartLine;
    actualEndLine = matchEndLine;
    isPartial = true;

    if (matchingLines.length > 1) {
      // List ALL match line numbers so the agent can issue targeted startLine/endLine reads
      const otherLines = matchingLines.slice(1);
      matchLocationsSet.add(
        `Found "${matchString}" on line ${firstMatch} (showing ±${matchStringContextLines} lines). Other occurrences at lines: ${otherLines.join(', ')} — use startLine/endLine to read those locations directly.`
      );
    } else {
      matchLocationsSet.add(`Found "${matchString}" on line ${firstMatch}`);
    }
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
  finalContent = applyContentViewMinification(
    sanitizationResult.content,
    filePath
  );

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
  // guide the agent to use startLine for tail access and signaturesOnly for an
  // export index — avoids agents giving up on large files they can already read.
  if (
    totalLines > 2000 &&
    !signaturesOnly &&
    !matchString &&
    !startLine &&
    !endLine &&
    !fullContent
  ) {
    const tailLine = Math.max(1, totalLines - 200);
    matchLocationsSet.add(
      `Large file (${totalLines} lines) — signaturesOnly=true for an export index, or startLine=${tailLine} for the tail.`
    );
  }

  const matchLocations = Array.from(matchLocationsSet);

  return {
    owner,
    repo,
    path: filePath,
    content: finalContent,
    branch,
    totalLines,
    ...(isPartial && {
      startLine: actualStartLine,
      endLine: actualEndLine,
      isPartial,
    }),
    ...(matchLocations.length > 0 && {
      matchLocations,
      warnings: matchLocations,
    }),
  } as GitHubFileContentApiResult;
}
