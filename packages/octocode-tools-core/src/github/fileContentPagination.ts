import type { GitHubFileContentApiResult } from '../tools/github_fetch_content/types.js';
import { getOutputCharLimit } from '../utils/pagination/charLimit.js';
import { GITHUB_FILE_CONTENT_DEFAULT_CHAR_LENGTH } from '../config.js';
import { applyPagination } from '../utils/pagination/core.js';
import {
  snapToSemanticBoundary,
  isMidBlockCut,
  findNextBlockBoundary,
} from '../utils/pagination/boundary.js';
import { OctokitWithThrottling } from './client.js';

function getDefaultContentPageSize(): number {
  const globalLimit = getOutputCharLimit();
  return Math.min(globalLimit, GITHUB_FILE_CONTENT_DEFAULT_CHAR_LENGTH);
}

interface FileTimestampInfo {
  lastModified: string;
  lastModifiedBy: string;
}

export async function applyContentPagination(
  data: GitHubFileContentApiResult,
  charOffset: number,
  charLength?: number
): Promise<GitHubFileContentApiResult> {
  const content = data.content ?? '';
  const maxChars = charLength ?? getDefaultContentPageSize();

  if (content.length <= maxChars && charOffset === 0) {
    return data;
  }

  const filePath = data.path ?? undefined;
  const { length: snappedLength, chunkMode } = await snapToSemanticBoundary(
    content,
    charOffset,
    maxChars,
    filePath
  );

  const paginationMeta = applyPagination(content, charOffset, snappedLength, {
    // snappedLength is snapped to a semantic boundary and varies per page; use
    // the stable requested page size (maxChars) for an absolute page counter —
    // same fix as local_fetch_content/fetchContent.ts's paginateContentWindow.
    pageSize: maxChars,
  });

  let nextBlockChar: number | undefined;
  if (paginationMeta.hasMore && chunkMode === 'char-limit') {
    if (isMidBlockCut(paginationMeta.paginatedContent)) {
      const cutPos = paginationMeta.charOffset + paginationMeta.charLength;
      nextBlockChar = await findNextBlockBoundary(content, cutPos, filePath);
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
      // `nextCharOffset` is the schema-promised cursor ("take charOffset from
      // pagination.nextCharOffset; don't compute it yourself"). applyPagination
      // already computes it; preserve it so the finalizer's buildContinueChars
      // can emit next.continueChars. Dropping it broke that continuation.
      ...(paginationMeta.nextCharOffset !== undefined && {
        nextCharOffset: paginationMeta.nextCharOffset,
      }),
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
