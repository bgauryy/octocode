import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { GitHubReposSearchBulkQueryLocalSchema } from '../../scheme/remoteSchemaOverlay.js';
import { searchMultipleGitHubRepos } from './execution.js';
import { GitHubSearchRepositoriesOutputSchema } from '@octocodeai/octocode-core';
import { createRemoteToolRegistration } from '../registerRemoteTool.js';

export const registerSearchGitHubReposTool = createRemoteToolRegistration({
  name: TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
  title: 'GitHub Repository Search',
  inputSchema: GitHubReposSearchBulkQueryLocalSchema,
  outputSchema: GitHubSearchRepositoriesOutputSchema,
  executionFn: searchMultipleGitHubRepos,
});
