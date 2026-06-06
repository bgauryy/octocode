export interface OAuthToken {
  token: string;
  tokenType: 'oauth';
  scopes?: string[];
  refreshToken?: string;
  expiresAt?: string;
  refreshTokenExpiresAt?: string;
}

export interface StoredCredentials {
  hostname: string;
  username: string;
  token: OAuthToken;
  gitProtocol: 'ssh' | 'https';
  createdAt: string;
  updatedAt: string;
}

export interface StoreResult {
  success: boolean;
}

export interface DeleteResult {
  success: boolean;
  deletedFromFile: boolean;
}

export interface CredentialsStore {
  version: number;
  credentials: Record<string, StoredCredentials>;
}

export type TokenSource =
  | 'env:OCTOCODE_TOKEN'
  | 'env:GH_TOKEN'
  | 'env:GITHUB_TOKEN'
  | 'octocode-storage'
  | 'gh-cli'
  | null;
