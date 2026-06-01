import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import {
  GitHubViewRepoStructureBulkQueryLocalSchema,
  GitHubViewRepoStructureOutputLocalSchema,
} from '../../scheme/remoteSchemaOverlay.js';
import { exploreMultipleRepositoryStructures } from './execution.js';
import { createRemoteToolRegistration } from '../registerRemoteTool.js';

// Tool description lives in a single canonical place — the host resource
// `octocode-mcp-host/src/octocode/resources/tools/githubViewRepoStructure.ts` —
// and is pulled through DESCRIPTIONS at registration time. Listing-level hints
// (flag/mode/config-file surface, empty-listing recovery) live in `./hints.ts`.
export const registerViewGitHubRepoStructureTool = createRemoteToolRegistration(
  {
    name: TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
    title: 'GitHub Repository Structure Explorer',
    inputSchema: GitHubViewRepoStructureBulkQueryLocalSchema,
    outputSchema: GitHubViewRepoStructureOutputLocalSchema,
    executionFn: exploreMultipleRepositoryStructures,
    describe: base => `${base}
<vsGithubGetFileContent>
- Use githubViewRepoStructure to inspect a remote tree cheaply without cloning.
- Use githubGetFileContent type="directory" when you need files materialized on disk for local/LSP tools.
</vsGithubGetFileContent>`,
  }
);
