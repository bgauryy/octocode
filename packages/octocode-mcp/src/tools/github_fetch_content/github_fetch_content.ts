import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { FileContentBulkQueryLocalSchema } from '../../scheme/remoteSchemaOverlay.js';
import { fetchMultipleGitHubFileContents } from './execution.js';
import { GitHubFetchContentOutputSchema } from '@octocodeai/octocode-core';
import { createRemoteToolRegistration } from '../registerRemoteTool.js';

export const registerFetchGitHubFileContentTool = createRemoteToolRegistration({
  name: TOOL_NAMES.GITHUB_FETCH_CONTENT,
  title: 'GitHub File Content Fetch',
  inputSchema: FileContentBulkQueryLocalSchema,
  outputSchema: GitHubFetchContentOutputSchema,
  executionFn: fetchMultipleGitHubFileContents,
  annotations: { readOnlyHint: false },
});
