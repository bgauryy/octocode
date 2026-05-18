import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { GitHubCodeSearchBulkQueryLocalSchema } from '../../scheme/remoteSchemaOverlay.js';
import { searchMultipleGitHubCode } from './execution.js';
import { GitHubSearchCodeOutputSchema } from '@octocodeai/octocode-core';
import { createRemoteToolRegistration } from '../registerRemoteTool.js';

export const registerGitHubSearchCodeTool = createRemoteToolRegistration({
  name: TOOL_NAMES.GITHUB_SEARCH_CODE,
  title: 'GitHub Code Search',
  inputSchema: GitHubCodeSearchBulkQueryLocalSchema,
  outputSchema: GitHubSearchCodeOutputSchema,
  executionFn: searchMultipleGitHubCode,
});
