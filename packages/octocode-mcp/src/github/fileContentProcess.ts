import type { GitHubFileContentApiResult } from '../tools/github_fetch_content/types.js';
import { getOutputCharLimit } from '../utils/pagination/charLimit.js';
import { ContentSanitizer } from 'octocode-security-utils/contentSanitizer';
import {
  applyPagination,
  createPaginationInfo,
} from '../utils/pagination/core.js';
import { generateGitHubPaginationHints } from '../utils/pagination/hints.js';
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

  const paginationHints = generateGitHubPaginationHints(paginationInfo, {
    owner: data.owner ?? '',
    repo: data.repo ?? '',
    path: data.path ?? '',
    branch: data.branch,
  });

  return {
    ...data,
    content: paginationMeta.paginatedContent,
    pagination: paginationInfo,
    hints: paginationHints,
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
  matchString?: string
): Promise<GitHubFileContentApiResult> {
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

    matchLocationsSet.add(
      `Found "${matchString}" on line ${firstMatch}${matchingLines.length > 1 ? ` (and ${matchingLines.length - 1} other locations)` : ''}`
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
  finalContent = sanitizationResult.content;

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
