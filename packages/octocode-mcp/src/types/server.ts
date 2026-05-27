/**
 * Server configuration & token-source types.
 *
 * @module types/server
 */

/**
 * Token source types for tracking where the GitHub token came from.
 */
export type TokenSourceType =
  | 'env:OCTOCODE_TOKEN'
  | 'env:GH_TOKEN'
  | 'env:GITHUB_TOKEN'
  | 'gh-cli'
  | 'octocode-storage'
  | 'none';

/** Server configuration and feature flags. */
export interface ServerConfig {
  version: string;
  githubApiUrl: string;
  toolsToRun?: string[];
  enableTools?: string[];
  disableTools?: string[];
  timeout: number;
  maxRetries: number;
  loggingEnabled: boolean;
  enableLocal: boolean;
  /** Whether clone/fetch repository functionality is enabled (requires enableLocal) */
  enableClone: boolean;
  /** Response serialization format: 'yaml' (default, token-efficient) or 'json' */
  outputFormat: 'yaml' | 'json';
  tokenSource: TokenSourceType;
}
