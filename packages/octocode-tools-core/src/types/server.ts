export type TokenSourceType =
  | 'env:OCTOCODE_TOKEN'
  | 'env:GH_TOKEN'
  | 'env:GITHUB_TOKEN'
  | 'gh-cli'
  | 'octocode-storage'
  | 'none';

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

  enableClone: boolean;

  outputFormat: 'yaml' | 'json';
  tokenSource: TokenSourceType;
}
