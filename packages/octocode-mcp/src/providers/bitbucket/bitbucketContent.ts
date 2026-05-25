import type {
  FileContentQuery,
  FileContentResult,
  ProviderResponse,
} from '../types.js';
import {
  fetchBitbucketFileContentAPI,
  getBitbucketDefaultBranch,
} from '../../bitbucket/fileContent.js';
import type { BitbucketFileContentResult } from '../../bitbucket/types.js';
import {
  handleBitbucketAPIResponse,
  parseBitbucketProjectId,
} from './utils.js';
import { extractFileContent } from '../contentExtraction.js';

export function transformFileContentResult(
  data: BitbucketFileContentResult,
  query: FileContentQuery
): FileContentResult {
  const extracted = extractFileContent(data.content, query);

  return {
    path: data.path || query.path,
    content: extracted.content,
    encoding: 'utf-8',
    size: data.size || 0,
    totalLines: extracted.totalLines,
    ref: data.ref || query.ref || '',
    lastCommitSha: data.lastCommitSha,
    pagination: extracted.pagination,
    isPartial: extracted.isPartial,
    startLine: extracted.startLine,
    endLine: extracted.endLine,
  };
}

export async function getFileContent(
  query: FileContentQuery
): Promise<ProviderResponse<FileContentResult>> {
  const { workspace, repoSlug } = parseBitbucketProjectId(query.projectId);

  let ref = query.ref;
  if (!ref) {
    ref = await getBitbucketDefaultBranch(workspace, repoSlug);
  }

  const result = await fetchBitbucketFileContentAPI({
    workspace,
    repoSlug,
    path: query.path,
    ref,
  });

  return handleBitbucketAPIResponse(result, data =>
    transformFileContentResult(data, query)
  );
}
