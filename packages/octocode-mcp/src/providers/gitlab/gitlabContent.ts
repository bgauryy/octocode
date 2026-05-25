/**
 * GitLab File Content Fetching
 *
 * Extracted from GitLabProvider for better modularity.
 *
 * @module providers/gitlab/gitlabContent
 */

import type {
  ProviderResponse,
  FileContentQuery,
  FileContentResult,
} from '../types.js';

import { fetchGitLabFileContentAPI } from '../../gitlab/fileContent.js';
import type {
  GitLabFileContentQuery,
  GitLabFileContent,
} from '../../gitlab/types.js';
import { handleGitLabAPIResponse, parseGitLabProjectId } from './utils.js';
import { extractFileContent } from '../contentExtraction.js';
export { parseGitLabProjectId };

/**
 * Transform GitLab file content result to unified format.
 */
export function transformFileContentResult(
  data: GitLabFileContent,
  query: FileContentQuery
): FileContentResult {
  const extracted = extractFileContent(data.content || '', query);

  return {
    path: data.file_path || query.path,
    content: extracted.content,
    encoding: (data.encoding === 'base64' ? 'base64' : 'utf-8') as
      | 'utf-8'
      | 'base64',
    size: data.size || 0,
    totalLines: extracted.totalLines,
    ref: data.ref || query.ref || '',
    lastCommitSha: data.last_commit_id,
    lastModifiedBy: undefined, // GitLab doesn't provide this in file content API
    pagination: extracted.pagination,
    isPartial: extracted.isPartial,
    startLine: extracted.startLine,
    endLine: extracted.endLine,
  };
}

/**
 * Get file content from GitLab.
 */
export async function getFileContent(
  query: FileContentQuery,
  parseProjectId: (projectId?: string) => number | string = parseGitLabProjectId
): Promise<ProviderResponse<FileContentResult>> {
  const projectId = parseProjectId(query.projectId);

  const gitlabQuery: GitLabFileContentQuery = {
    projectId,
    path: query.path,
    ref: query.ref || 'HEAD',
    startLine: query.startLine,
    endLine: query.endLine,
  };

  const result = await fetchGitLabFileContentAPI(gitlabQuery);
  return handleGitLabAPIResponse(
    result,
    'gitlab',
    data => transformFileContentResult(data, query),
    { stringifyError: true }
  );
}
