import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import {
  FileContentBulkQueryLocalSchema,
  GitHubFetchContentOutputLocalSchema,
} from '../../scheme/remoteSchemaOverlay.js';
import { fetchMultipleGitHubFileContents } from './execution.js';
import { createRemoteToolRegistration } from '../registerRemoteTool.js';

export const registerFetchGitHubFileContentTool = createRemoteToolRegistration({
  name: TOOL_NAMES.GITHUB_FETCH_CONTENT,
  title: 'GitHub File Content Fetch',
  inputSchema: FileContentBulkQueryLocalSchema,
  outputSchema: GitHubFetchContentOutputLocalSchema,
  executionFn: fetchMultipleGitHubFileContents,
  annotations: { readOnlyHint: false },
});
