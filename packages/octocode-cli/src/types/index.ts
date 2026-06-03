export type ColorName =
  | 'reset'
  | 'bright'
  | 'dim'
  | 'underscore'
  | 'red'
  | 'green'
  | 'yellow'
  | 'blue'
  | 'magenta'
  | 'cyan'
  | 'white'
  | 'bgRed'
  | 'bgGreen'
  | 'bgYellow'
  | 'bgBlue'
  | 'bgMagenta';

export interface MCPServer {
  command?: string;
  args?: string[];

  url?: string;

  env?: Record<string, string>;
}

export interface MCPConfig {
  mcpServers?: Record<string, MCPServer>;
}

export type MCPClient =
  | 'cursor'
  | 'claude-desktop'
  | 'claude-code'
  | 'vscode-cline'
  | 'vscode-roo'
  | 'vscode-continue'
  | 'windsurf'
  | 'trae'
  | 'antigravity'
  | 'zed'
  | 'opencode'
  | 'codex'
  | 'gemini-cli'
  | 'goose'
  | 'kiro'
  | 'custom';

export type IDE = Exclude<MCPClient, 'custom'> | 'claude';

export type MCPClientCategory = 'ide' | 'desktop' | 'extension' | 'cli';

export interface MCPClientInfo {
  id: MCPClient;
  name: string;
  description: string;
  category: MCPClientCategory;
  url?: string;
  envVars?: string[];
}

export type InstallMethod = 'npx';

export interface GitHubAuthStatus {
  installed: boolean;
  authenticated: boolean;
  username?: string;
  error?: string;
}

export type { OAuthToken, StoredCredentials } from 'octocode-shared';

export type TokenSource = 'octocode' | 'gh-cli' | 'env' | 'none';

export interface OctocodeAuthStatus {
  authenticated: boolean;
  hostname?: string;
  username?: string;
  tokenExpired?: boolean;
  tokenSource?: TokenSource;

  envTokenSource?: string;
  error?: string;
}

export interface TokenResult {
  token: string | null;
  source: TokenSource;
  username?: string;

  envSource?: string;
}
