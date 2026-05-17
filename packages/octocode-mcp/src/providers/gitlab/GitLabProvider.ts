/**
 * GitLab Provider Adapter
 *
 * Implements the ICodeHostProvider interface by wrapping GitLab API functions.
 * This adapter transforms unified query/result types to/from GitLab-specific formats.
 *
 * @module providers/gitlab/GitLabProvider
 */

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

import * as gitlabSearch from './gitlabSearch.js';
import * as gitlabContent from './gitlabContent.js';
import * as gitlabPullRequests from './gitlabPullRequests.js';
import * as gitlabStructure from './gitlabStructure.js';

import { handleGitLabAPIError } from '../../gitlab/errors.js';
import type { GitLabAPIError } from '../../gitlab/types.js';
import { getGitlab } from '../../gitlab/client.js';
import { parseGitLabDefaultBranch } from '../../gitlab/responseGuards.js';
import { logRateLimit } from '../../session.js';
import { PROVIDER_CAPABILITIES } from '../capabilities.js';
import {
  mapGitLabMRState,
  mapGitLabRepoSortField,
  parseGitLabProjectId,
} from './utils.js';

/**
 * GitLab Provider implementation.
 *
 * Wraps GitLab API functions to conform to the unified ICodeHostProvider interface.
 */
export class GitLabProvider implements ICodeHostProvider {
  readonly type = 'gitlab' as const;
  readonly capabilities = PROVIDER_CAPABILITIES.gitlab;
  private config?: ProviderConfig;

  constructor(config?: ProviderConfig) {
    this.config = config;
  }

  async searchCode(
    query: CodeSearchQuery
  ): Promise<ProviderResponse<CodeSearchResult>> {
    try {
      return await gitlabSearch.searchCode(query, parseGitLabProjectId);
    } catch (error) {
      return this.handleError(error);
    }
  }

  async getFileContent(
    query: FileContentQuery
  ): Promise<ProviderResponse<FileContentResult>> {
    try {
      return await gitlabContent.getFileContent(query, parseGitLabProjectId);
    } catch (error) {
      return this.handleError(error);
    }
  }

  async searchRepos(
    query: RepoSearchQuery
  ): Promise<ProviderResponse<RepoSearchResult>> {
    try {
      return await gitlabSearch.searchRepos(query, mapGitLabRepoSortField);
    } catch (error) {
      return this.handleError(error);
    }
  }

  async searchPullRequests(
    query: PullRequestQuery
  ): Promise<ProviderResponse<PullRequestSearchResult>> {
    try {
      return await gitlabPullRequests.searchPullRequests(
        query,
        parseGitLabProjectId,
        mapGitLabMRState
      );
    } catch (error) {
      return this.handleError(error);
    }
  }

  async getRepoStructure(
    query: RepoStructureQuery
  ): Promise<ProviderResponse<RepoStructureResult>> {
    try {
      return await gitlabStructure.getRepoStructure(
        query,
        parseGitLabProjectId
      );
    } catch (error) {
      return this.handleError(error);
    }
  }

  async resolveDefaultBranch(projectId: string): Promise<string> {
    try {
      const gitlab = await getGitlab(
        this.config?.baseUrl || this.config?.token
          ? { host: this.config.baseUrl, token: this.config.token }
          : undefined
      );
      const parsedId = parseGitLabProjectId(projectId);
      const branch = parseGitLabDefaultBranch(
        await gitlab.Projects.show(parsedId)
      );
      return branch || 'main';
    } catch {
      return 'main';
    }
  }

  private handleError(error: unknown): ProviderResponse<never> {
    const apiError = handleGitLabAPIError(error);
    const rateLimit = this.extractRateLimit(apiError);

    if (rateLimit) {
      void logRateLimit({
        limit_type: 'primary',
        retry_after_seconds: rateLimit.retryAfter,
        rate_limit_remaining: rateLimit.remaining,
        rate_limit_reset_ms: rateLimit.reset * 1000,
        provider: 'gitlab',
      });
    }

    return {
      error: apiError.error,
      status: apiError.status || 500,
      provider: 'gitlab',
      hints: apiError.hints,
      rateLimit,
    };
  }

  private extractRateLimit(
    apiError: GitLabAPIError
  ): ProviderResponse<never>['rateLimit'] {
    if (
      apiError.rateLimitRemaining === undefined &&
      apiError.retryAfter === undefined &&
      apiError.rateLimitReset === undefined
    ) {
      return undefined;
    }

    const reset =
      apiError.rateLimitReset ??
      (apiError.retryAfter !== undefined
        ? Math.floor(Date.now() / 1000) + apiError.retryAfter
        : undefined);

    if (reset === undefined) {
      return undefined;
    }

    return {
      remaining: apiError.rateLimitRemaining ?? 0,
      reset,
      retryAfter: apiError.retryAfter,
    };
  }
}
