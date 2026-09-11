import type { AuthInfo } from '@modelcontextprotocol/server';
import type {
  ICodeHostProvider,
  ProviderConfig,
  ProviderResponse,
} from '../types.js';
import type {
  CodeSearchQuery,
  FileContentQuery,
  RepoSearchQuery,
  PullRequestQuery,
  RepoStructureQuery,
} from '../providerQueries.js';
import type {
  CodeSearchResult,
  FileContentResult,
  RepoSearchResult,
  PullRequestSearchResult,
  RepoStructureResult,
} from '../providerResults.js';

import * as githubSearch from './githubSearch.js';
import * as githubContent from './githubContent.js';
import * as githubPullRequests from './githubPullRequests.js';
import * as githubStructure from './githubStructure.js';

import { handleGitHubAPIError } from '../../github/errors.js';
import { resolveDefaultBranch as resolveGitHubDefaultBranch } from '../../github/client.js';
import { PROVIDER_CAPABILITIES } from '../capabilities.js';
import { createGitHubProviderError, parseGitHubProjectId } from './utils.js';
import { fetchIssues } from '../../github/issues/orchestrator.js';
import { searchCommits } from '../../github/commitSearch.js';
import { fetchHistory } from '../../github/history.js';
import { compareRefs } from '../../github/compare.js';
import { fetchCommit } from '../../github/commit.js';

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

  fetchIssues(query: Parameters<typeof fetchIssues>[0]) {
    return fetchIssues(query, this.authInfo);
  }

  searchCommits(query: Parameters<typeof searchCommits>[0]) {
    return searchCommits(query, this.authInfo);
  }

  fetchHistory(query: Parameters<typeof fetchHistory>[0]) {
    return fetchHistory(query, this.authInfo);
  }

  compareRefs(query: Parameters<typeof compareRefs>[0]) {
    return compareRefs(query, this.authInfo);
  }

  fetchCommit(query: Parameters<typeof fetchCommit>[0]) {
    return fetchCommit(query, this.authInfo);
  }

  private handleError(error: unknown): ProviderResponse<never> {
    const apiError = handleGitHubAPIError(error);
    return createGitHubProviderError(apiError);
  }
}
