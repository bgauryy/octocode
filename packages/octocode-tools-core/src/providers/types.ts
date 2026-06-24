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

export type ProviderType = 'github';

export interface ProviderConfig {
  type: ProviderType;

  baseUrl?: string;

  token?: string;

  authInfo?: AuthInfo;
}

export interface ProviderCapabilities {
  cloneRepo: boolean;
  fetchDirectoryToDisk: boolean;
  requiresScopedCodeSearch: boolean;
  supportsMergedState: boolean;
  supportsMultiTopicSearch: boolean;
}

export interface ProviderResponse<T> {
  data?: T;

  error?: string;

  status: number;

  provider: ProviderType;

  hints?: string[];

  rateLimit?: {
    remaining: number;

    reset: number;
    retryAfter?: number;
  };

  rawResponseChars?: number;
}

export interface ICodeHostProvider {
  readonly type: ProviderType;

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

  resolveDefaultBranch(projectId: string): Promise<string>;
}

export function isProviderSuccess<T>(
  response: ProviderResponse<T>
): response is ProviderResponse<T> & { data: T } {
  return response.data !== undefined && !response.error;
}

export function isProviderError<T>(
  response: ProviderResponse<T>
): response is ProviderResponse<T> & { error: string } {
  return response.error !== undefined;
}
