import type { MCPClient } from '../../../types/index.js';
import type { ClientInstallStatus } from '../../../utils/mcp-config.js';

export interface ClientChoice {
  name: string;
  value: MCPClient | 'back' | 'install-new';
  disabled?: boolean | string;
}

export interface ClientWithStatus {
  clientId: MCPClient;
  status: ClientInstallStatus;
  isAvailable: boolean;
}

export type LocalToolsChoice = 'enable' | 'disable' | 'back';

export type GitHubAuthMethod = 'gh-cli' | 'token' | 'skip' | 'back';
