import type { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types.js';
import type {
  ICodeHostProvider,
  ProviderConfig,
  ProviderResponse,
  CodeSearchQuery,
  CodeSearchResult,
  FileContentQuery,
  FileContentResult,
  RepoSearchQuery,
  RepoSearchResult,
  PullRequestQuery,
  PullRequestSearchResult,
  RepoStructureQuery,
  RepoStructureResult,
} from '../types.js';

import * as githubSearch from './githubSearch.js';
import * as githubContent from './githubContent.js';
import * as githubPullRequests from './githubPullRequests.js';
import * as githubStructure from './githubStructure.js';

import { handleGitHubAPIError } from '../../github/errors.js';
import { resolveDefaultBranch as resolveGitHubDefaultBranch } from '../../github/client.js';
import { PROVIDER_CAPABILITIES } from '../capabilities.js';
import { createGitHubProviderError, parseGitHubProjectId } from './utils.js';

export class GitHubProvider implements ICodeHostProvider {
  readonly type = 'github' as const;
  readonly capabilities = PROVIDER_CAPABILITIES.github;
  private authInfo?: AuthInfo;

  constructor(config?: ProviderConfig) {
    if (config?.authInfo) {
      this.authInfo = config.authInfo;
    } else if (config?.token) {
      this.authInfo = { token: config.token } as AuthInfo;
    }
  }

  async searchCode(
    query: CodeSearchQuery
  ): Promise<ProviderResponse<CodeSearchResult>> {
    try {
      return await githubSearch.searchCode(
        query,
        this.authInfo,
        parseGitHubProjectId
      );
    } catch (error) {
      return this.handleError(error);
    }
  }

  async getFileContent(
    query: FileContentQuery
  ): Promise<ProviderResponse<FileContentResult>> {
    try {
      return await githubContent.getFileContent(
        query,
        this.authInfo,
        parseGitHubProjectId
      );
    } catch (error) {
      return this.handleError(error);
    }
  }

  async searchRepos(
    query: RepoSearchQuery
  ): Promise<ProviderResponse<RepoSearchResult>> {
    try {
      return await githubSearch.searchRepos(query, this.authInfo);
    } catch (error) {
      return this.handleError(error);
    }
  }

  async searchPullRequests(
    query: PullRequestQuery
  ): Promise<ProviderResponse<PullRequestSearchResult>> {
    try {
      return await githubPullRequests.searchPullRequests(
        query,
        this.authInfo,
        parseGitHubProjectId
      );
    } catch (error) {
      return this.handleError(error);
    }
  }

  async getRepoStructure(
    query: RepoStructureQuery
  ): Promise<ProviderResponse<RepoStructureResult>> {
    try {
      return await githubStructure.getRepoStructure(
        query,
        this.authInfo,
        parseGitHubProjectId
      );
    } catch (error) {
      return this.handleError(error);
    }
  }

  async resolveDefaultBranch(projectId: string): Promise<string> {
    const { owner, repo } = parseGitHubProjectId(projectId);
    if (!owner || !repo) {
      throw new Error(
        `Cannot resolve default branch: invalid projectId '${projectId}'.`
      );
    }
    return resolveGitHubDefaultBranch(owner, repo, this.authInfo);
  }

  private handleError(error: unknown): ProviderResponse<never> {
    const apiError = handleGitHubAPIError(error);
    return createGitHubProviderError(apiError);
  }
}
