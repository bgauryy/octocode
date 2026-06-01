import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import {
  FileContentBulkQueryLocalSchema,
  GitHubFetchContentOutputLocalSchema,
} from '../../scheme/remoteSchemaOverlay.js';
import { fetchMultipleGitHubFileContents } from './execution.js';
import { createRemoteToolRegistration } from '../registerRemoteTool.js';

// Tool description lives in a single canonical place — the host resource
// `octocode-mcp-host/src/octocode/resources/tools/githubGetFileContent.ts` —
// and is pulled through DESCRIPTIONS at registration time. Response-state
// guidance (partial-content cursor, non-canonical-path warning, not-found
// recovery) lives in `./hints.ts`.
export const registerFetchGitHubFileContentTool = createRemoteToolRegistration({
  name: TOOL_NAMES.GITHUB_FETCH_CONTENT,
  title: 'GitHub File Content Fetch',
  inputSchema: FileContentBulkQueryLocalSchema,
  outputSchema: GitHubFetchContentOutputLocalSchema,
  executionFn: fetchMultipleGitHubFileContents,
  describe: base => `${base}
<directoryMode>type="directory" requires ENABLE_LOCAL=true and ENABLE_CLONE=true. If disabled, use file mode or enable clone before expecting githubCloneRepo/directory fetch to appear.</directoryMode>`,
  annotations: { readOnlyHint: false },
});
