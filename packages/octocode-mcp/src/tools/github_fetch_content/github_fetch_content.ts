import {
  TOOL_NAMES,
  FileContentBulkQueryLocalSchema,
  GitHubFetchContentOutputLocalSchema,
  fetchMultipleGitHubFileContents,
} from '@octocodeai/octocode-tools-core';
import { createRemoteToolRegistration } from '../registerRemoteTool.js';

export const registerFetchGitHubFileContentTool = createRemoteToolRegistration({
  name: TOOL_NAMES.GITHUB_FETCH_CONTENT,
  title: 'GitHub File Content Fetch',
  inputSchema: FileContentBulkQueryLocalSchema,
  outputSchema: GitHubFetchContentOutputLocalSchema,
  executionFn: fetchMultipleGitHubFileContents,
});
