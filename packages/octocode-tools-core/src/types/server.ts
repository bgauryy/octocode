export type TokenSourceType =
  | 'env:OCTOCODE_TOKEN'
  | 'env:GH_TOKEN'
  | 'env:GITHUB_TOKEN'
  | 'env:GITHUB_PERSONAL_ACCESS_TOKEN'
  | 'gh-cli'
  | 'octocode-storage'
  | 'none';

export interface ServerConfig {
  version: string;
  githubApiUrl: string;
  toolsToRun?: string[];
  disableTools?: string[];
  timeout: number;
  maxRetries: number;
  enableLocal: boolean;

  enableClone: boolean;

  tokenSource: TokenSourceType;
}
