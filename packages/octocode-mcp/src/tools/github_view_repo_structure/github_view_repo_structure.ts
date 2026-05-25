import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import {
  GitHubViewRepoStructureBulkQueryLocalSchema,
  GitHubViewRepoStructureOutputLocalSchema,
} from '../../scheme/remoteSchemaOverlay.js';
import { exploreMultipleRepositoryStructures } from './execution.js';
import { createRemoteToolRegistration } from '../registerRemoteTool.js';

export const registerViewGitHubRepoStructureTool = createRemoteToolRegistration(
  {
    name: TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
    title: 'GitHub Repository Structure Explorer',
    inputSchema: GitHubViewRepoStructureBulkQueryLocalSchema,
    outputSchema: GitHubViewRepoStructureOutputLocalSchema,
    executionFn: exploreMultipleRepositoryStructures,
  }
);
