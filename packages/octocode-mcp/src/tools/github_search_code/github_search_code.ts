import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import {
  GitHubCodeSearchBulkQueryLocalSchema,
  GitHubCodeSearchOutputLocalSchema,
} from '../../scheme/remoteSchemaOverlay.js';
import { searchMultipleGitHubCode } from './execution.js';
import { createRemoteToolRegistration } from '../registerRemoteTool.js';

// Note: empty-result recovery hints (multi-filter overload, AND-logic warning,
// match-mode switch, package-name redirect) now live in
// `./hints.ts:empty` so they fire only when actually empty, not every call.
// The description is left as the upstream text alone.
export const registerGitHubSearchCodeTool = createRemoteToolRegistration({
  name: TOOL_NAMES.GITHUB_SEARCH_CODE,
  title: 'GitHub Code Search',
  inputSchema: GitHubCodeSearchBulkQueryLocalSchema,
  outputSchema: GitHubCodeSearchOutputLocalSchema,
  executionFn: searchMultipleGitHubCode,
});
