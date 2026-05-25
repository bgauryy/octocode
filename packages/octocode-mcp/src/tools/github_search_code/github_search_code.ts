import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import {
  GitHubCodeSearchBulkQueryLocalSchema,
  GitHubCodeSearchOutputLocalSchema,
} from '../../scheme/remoteSchemaOverlay.js';
import { searchMultipleGitHubCode } from './execution.js';
import { createRemoteToolRegistration } from '../registerRemoteTool.js';

// Note: empty-result recovery hints (multi-filter overload, AND-logic warning,
// match-mode switch, package-name redirect) live in `./hints.ts:empty`.
// hasResults pagination warning lives in `./hints.ts:hasResults`.
// The describe() callback appends a <local_gotchas> block with patterns
// discovered from benchmark analysis (Q2, Q4, Q21 failure modes).
export const registerGitHubSearchCodeTool = createRemoteToolRegistration({
  name: TOOL_NAMES.GITHUB_SEARCH_CODE,
  title: 'GitHub Code Search',
  inputSchema: GitHubCodeSearchBulkQueryLocalSchema,
  outputSchema: GitHubCodeSearchOutputLocalSchema,
  executionFn: searchMultipleGitHubCode,
  describe: base =>
    base +
    `
  <gotchas>
  - All call sites: limit=30 + paginate until hasMore=false (default 10 misses later pages)
  - Type/interface names beat generic function names for precision in multi-version repos
  - Related concepts live in separate files — bulk-query both in one call (primitive + tracker, producer + consumer)
  - One match → browse its parent dir (githubViewRepoStructure) to find siblings
  </gotchas>`,
});
