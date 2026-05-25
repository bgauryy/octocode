import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import {
  FileContentBulkQueryLocalSchema,
  GitHubFetchContentOutputLocalSchema,
} from '../../scheme/remoteSchemaOverlay.js';
import { fetchMultipleGitHubFileContents } from './execution.js';
import { createRemoteToolRegistration } from '../registerRemoteTool.js';

// Static "how to use this tool" hints have been removed from the description
// and folded into either: (a) the upstream tool description (purpose + when +
// gotchas + examples), or (b) the dynamic empty/error/pagination branches in
// `./hints.ts`. No static-in-description hint flow remains.
export const registerFetchGitHubFileContentTool = createRemoteToolRegistration({
  name: TOOL_NAMES.GITHUB_FETCH_CONTENT,
  title: 'GitHub File Content Fetch',
  inputSchema: FileContentBulkQueryLocalSchema,
  outputSchema: GitHubFetchContentOutputLocalSchema,
  executionFn: fetchMultipleGitHubFileContents,
  annotations: { readOnlyHint: false },
});
