import {
  TOOL_NAMES,
  GitHubReposSearchBulkQueryLocalSchema,
  GitHubSearchRepositoriesOutputLocalSchema,
  searchMultipleGitHubRepos,
} from '@octocodeai/octocode-tools-core';
import { createRemoteToolRegistration } from '../registerRemoteTool.js';

export const registerSearchGitHubReposTool = createRemoteToolRegistration({
  name: TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
  title: 'GitHub Repository Search',
  inputSchema: GitHubReposSearchBulkQueryLocalSchema,
  outputSchema: GitHubSearchRepositoriesOutputLocalSchema,
  executionFn: searchMultipleGitHubRepos,
});
