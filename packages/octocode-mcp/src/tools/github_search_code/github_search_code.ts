import {
  TOOL_NAMES,
  GitHubCodeSearchBulkQueryLocalSchema,
  searchMultipleGitHubCode,
} from '@octocodeai/octocode-tools-core';
import { createRemoteToolRegistration } from '../registerRemoteTool.js';

export const registerGitHubSearchCodeTool = createRemoteToolRegistration({
  name: TOOL_NAMES.GITHUB_SEARCH_CODE,
  title: 'GitHub Code Search',
  inputSchema: GitHubCodeSearchBulkQueryLocalSchema,
  executionFn: searchMultipleGitHubCode,
});
