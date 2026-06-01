import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import {
  GitHubCodeSearchBulkQueryLocalSchema,
  GitHubCodeSearchOutputLocalSchema,
} from '../../scheme/remoteSchemaOverlay.js';
import { searchMultipleGitHubCode } from './execution.js';
import { createRemoteToolRegistration } from '../registerRemoteTool.js';

// Tool description lives in a single canonical place — the host resource
// `octocode-mcp-host/src/octocode/resources/tools/githubSearchCode.ts` —
// and is pulled through DESCRIPTIONS at registration time. Empty-result
// pivot hints live in `./hints.ts:empty`; non-canonical-path and pagination
// warnings live in `./hints.ts:hasResults`.
export const registerGitHubSearchCodeTool = createRemoteToolRegistration({
  name: TOOL_NAMES.GITHUB_SEARCH_CODE,
  title: 'GitHub Code Search',
  inputSchema: GitHubCodeSearchBulkQueryLocalSchema,
  outputSchema: GitHubCodeSearchOutputLocalSchema,
  executionFn: searchMultipleGitHubCode,
  describe: base => `${base}
<gotchas>
- keywordsToSearch is a narrowing set of terms, not a broad synonym list. For OR-style exploration, run separate queries.
- itemsPerPage (default 20) sets how many whole matches a page returns and drives GitHub per_page (fetched == shown); githubAPILimit overrides the raw per_page. Under verbosity="concise" the page is capped to 3.
</gotchas>`,
});
