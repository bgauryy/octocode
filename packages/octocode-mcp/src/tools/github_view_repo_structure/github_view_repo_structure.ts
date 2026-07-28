import {
  TOOL_NAMES,
  GitHubViewRepoStructureBulkQueryLocalSchema,
  exploreMultipleRepositoryStructures,
} from '@octocodeai/octocode-tools-core';
import { createRemoteToolRegistration } from '../registerRemoteTool.js';

export const registerViewGitHubRepoStructureTool = createRemoteToolRegistration(
  {
    name: TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
    title: 'GitHub Repository Structure Explorer',
    inputSchema: GitHubViewRepoStructureBulkQueryLocalSchema,
    executionFn: exploreMultipleRepositoryStructures,
  }
);
