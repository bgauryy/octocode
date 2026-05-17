/**
 * Provider Abstraction Layer - Type Definitions
 *
 * This module defines the interfaces for provider-agnostic code hosting operations.
 * Tools use these unified types, and the execution layer routes to the appropriate
 * provider (GitHub, GitLab, etc.) based on the `provider` parameter.
 *
 * @module providers/types
 */

import type { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types.js';

export type {
  CodeSearchQuery,
  FileContentQuery,
  RepoSearchQuery,
  PullRequestQuery,
  RepoStructureQuery,
} from './providerQueries.js';

export type {
  UnifiedRepository,
  CodeSearchItem,
  CodeSearchResult,
  FileContentResult,
  RepoSearchResult,
  PullRequestItem,
  PullRequestSearchResult,
  DirectoryEntry,
  RepoStructureResult,
} from './providerResults.js';

import type {
  CodeSearchQuery,
  FileContentQuery,
  RepoSearchQuery,
  PullRequestQuery,
  RepoStructureQuery,
} from './providerQueries.js';
import type {
  CodeSearchResult,
  FileContentResult,
  RepoSearchResult,
  PullRequestSearchResult,
  RepoStructureResult,
} from './providerResults.js';

/**
 * Supported code hosting providers.
 * Default is 'github' .
 */
export type ProviderType = 'github' | 'gitlab' | 'bitbucket';

/**
 * Configuration for provider initialization.
 */
export interface ProviderConfig {
  /** Provider type */
  type: ProviderType;
  /** Base URL for self-hosted instances (e.g., 'https://gitlab.mycompany.com') */
  baseUrl?: string;
  /** Provider-specific authentication token */
  token?: string;
  /** MCP auth info (contains OAuth token for GitHub) */
  authInfo?: AuthInfo;
}

/**
 * Declares provider-specific capabilities so tools can make behavior decisions
 * without hardcoding provider names.
 */
export interface ProviderCapabilities {
  cloneRepo: boolean;
  fetchDirectoryToDisk: boolean;
  requiresScopedCodeSearch: boolean;
  supportsMergedState: boolean;
  supportsMultiTopicSearch: boolean;
}

/**
 * Standardized response from provider operations.
 */
export interface ProviderResponse<T> {
  /** Response data (on success) */
  data?: T;
  /** Error message (on failure) */
  error?: string;
  /** HTTP status code */
  status: number;
  /** Provider that handled the request */
  provider: ProviderType;
  /** Additional hints for the user */
  hints?: string[];
  /** Rate limit info */
  rateLimit?: {
    remaining: number;
    /** Reset time in **seconds** (Unix timestamp). All providers normalize to this unit. */
    reset: number;
    retryAfter?: number;
  };
  /** Character count of the raw provider/source response before Octocode trimming. */
  rawResponseChars?: number;
}

/**
 * Interface that all code hosting providers must implement.
 */
export interface ICodeHostProvider {
  /** Provider type identifier */
  readonly type: ProviderType;
  /** Capability descriptor for tool-level flow decisions */
  readonly capabilities: ProviderCapabilities;

  searchCode(
    query: CodeSearchQuery
  ): Promise<ProviderResponse<CodeSearchResult>>;

  getFileContent(
    query: FileContentQuery
  ): Promise<ProviderResponse<FileContentResult>>;

  searchRepos(
    query: RepoSearchQuery
  ): Promise<ProviderResponse<RepoSearchResult>>;

  searchPullRequests(
    query: PullRequestQuery
  ): Promise<ProviderResponse<PullRequestSearchResult>>;

  getRepoStructure(
    query: RepoStructureQuery
  ): Promise<ProviderResponse<RepoStructureResult>>;

  /**
   * Resolve the default branch for a repository.
   * Each provider uses its own API to determine the default branch.
   *
   * @param projectId - Provider-specific project identifier (e.g., 'owner/repo')
   * @returns Default branch name (e.g., 'main', 'master')
   */
  resolveDefaultBranch(projectId: string): Promise<string>;
}

/**
 * Check if a response is successful.
 */
export function isProviderSuccess<T>(
  response: ProviderResponse<T>
): response is ProviderResponse<T> & { data: T } {
  return response.data !== undefined && !response.error;
}

/**
 * Check if a response is an error.
 */
export function isProviderError<T>(
  response: ProviderResponse<T>
): response is ProviderResponse<T> & { error: string } {
  return response.error !== undefined;
}
